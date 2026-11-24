<template>
  <MUpdateSheet
    v-model="visible"
    :phase="phase"
    :current="isLatest ? '' : currentVersion"
    :next="updateInfo?.version || ''"
    :channel="channel"
    :notes="sheet?.notes || null"
    :drivers="sheet?.drivers || null"
    :progress="download.progress"
    :downloaded="download.downloaded"
    :total="download.total || updateInfo?.download_size || 0"
    :source="download.source"
    :download-phase="download.downloadPhase"
    :install-stage="installStage"
    :install-detail="installDetail"
    :wait-hint="installWaitHint"
    :error-text="errorText"
    :error-step="errorStep"
    :mirror-host="mirrorHost"
    :has-release-page="!!sheet?.releasePage"
    :can-skip="!isLatest"
    :locale="locale"
    :close-label="t.updateDialog.close"
    @update="handleDownload"
    @restart="handleInstall(downloadedFilePath)"
    @later="handleCancel"
    @skip="handleSkipVersion"
    @retry="handleRetry"
    @open-release="handleOpenBrowser()"
    @open-changelog="handleOpenBrowser"
  >
    <template v-if="latestNote || fallbackNote || skippedNote || canUseGithub" #notice>
      <MBanner v-if="latestNote" tone="info" icon="info">{{ latestNote }}</MBanner>
      <MBanner v-if="fallbackNote" tone="info" icon="swap_horiz">{{ fallbackNote }}</MBanner>
      <MBanner v-if="skippedNote" tone="info" icon="sync_problem">{{ skippedNote }}</MBanner>
      <div v-if="canUseGithub" class="m-update-github">
        <MButton variant="outlined" icon="cloud_download" @click="handleDownloadFromGithub">{{ t.updateDialog.useGithub }}</MButton>
      </div>
    </template>
  </MUpdateSheet>
</template>

<script setup>
import { ref, computed, watch, onBeforeUnmount } from 'vue'
import { ElMessage, ElMessageBox } from '../composables/notifyMessage.js'
import { MBanner, MButton, MUpdateSheet, sheetFromPanelInfo } from 'foundation-ui/vue'
import { channelOfVersion, displayVersion as displayRelease, fetchReleaseMetadata, orderedDownloadUrls } from 'foundation-ui/update'
import { belongsToUpdate, downloadFailureText, githubUrlOf, isInstallerMismatch, skipNote } from '../composables/updateSources.js'
import { useUpdateSource } from '../composables/useUpdateSource.js'
import { mirrorFallbackText } from '../composables/mirrorFallback.js'
import { useUpdateDownload } from '../composables/updateDownload.js'
import { useI18n } from '../desktop/i18n/index.js'

const { t, locale } = useI18n()
const updateSource = useUpdateSource()

const DOWNLOAD_ERROR_MESSAGE_KEYS = Object.freeze({
  setup_failed: 'downloadSetupFailed',
  file_preparation_failed: 'downloadFilePreparationFailed',
  file_finalization_failed: 'downloadFileFinalizationFailed',
  sources_exhausted: 'downloadSourcesFailed',
  no_verified_source: 'downloadNoSource',
})

const props = defineProps({
  modelValue: { type: Boolean, default: false },
  updateInfo: { type: Object, default: null },
  currentVersion: { type: String, default: '0.0.0' },
})

const emit = defineEmits(['update:modelValue', 'close', 'skip-version'])

const visible = computed({
  get: () => props.modelValue,
  set: (val) => emit('update:modelValue', val),
})

const updateDownload = useUpdateDownload()
const isDownloading = computed(() => updateDownload.state.active)
const download = computed(() => updateDownload.state)
// the finished file counts only when it is the installer of the update this sheet offers: the native task keeps the LAST download, which can be an earlier release's
const downloadedFilePath = computed(() => {
  const { path, sha256, filename } = updateDownload.state
  return path && belongsToUpdate({ sha256, filename }, props.updateInfo) ? path : ''
})
const isInstalling = ref(false)
const installStage = ref('idle')
const installDetail = ref('')
const installWaitHint = ref('')
const installLastEventAt = ref(0)
const errorText = ref('')
const errorStep = ref('download')
const channel = ref('')
const mirrorHost = ref('')
// "改用 GitHub 下载": the person's choice for this update; Retry keeps it
const onlySource = ref('')
let installProgressUnlisten = null
let installWaitTimer = null

// the download did not come from Mirror酱 although it is the chosen source: one sentence with the reason and the source that took over
const fallbackNote = computed(() => mirrorFallbackText(updateDownload.state.fallbackReason, updateDownload.state.source, t.value.updateDialog.mirrorFallback))
// sources that were left out for this release (CNB or Mirror酱 not synced yet) and the one that took over
const skippedNote = computed(() => (updateDownload.state.version === props.updateInfo?.version
  ? skipNote(updateDownload.state.skipped, props.updateInfo?.version, updateDownload.state.source, { skip: t.value.updateDialog.sourceSkip, usedInstead: t.value.updateDialog.sourceUsedInstead })
  : ''))
// the way out of a failed download or install that does not depend on the mirrors: GitHub carries the release, and it was not the only thing tried
// (not when GitHub itself was left out for this release or was tried and failed: asking it again alone changes nothing)
const canUseGithub = computed(() => phase.value === 'error' && !!githubUrlOf(props.updateInfo) && onlySource.value !== 'github'
  && ![...updateDownload.state.skipped, ...updateDownload.state.failed].some((entry) => entry?.source === 'GitHub'))
const isLatest = computed(() => !!props.updateInfo?.is_latest)
// "up to date" plus what else is true: the installed version is a prerelease and nothing newer exists
const latestNote = computed(() => (isLatest.value && props.updateInfo?.latest_note === 'installed_prerelease_no_newer' ? t.value.updateDialog.latestNoteInstalledPrerelease : ''))

// Notes, driver notice, release page and installer facts of the update the host reported.
const sheet = computed(() => sheetFromPanelInfo(props.updateInfo, { current: props.currentVersion }))

const phase = computed(() => {
  if (isLatest.value) return 'latest'
  if (errorText.value) return 'error'
  if (isInstalling.value) return 'installing'
  if (isDownloading.value) return 'downloading'
  // 安装包已就绪：本次下载完成或上次会话已完整下载，可直接安装
  if (downloadedFilePath.value) return 'ready'
  return 'available'
})

const resetDownloadState = () => updateDownload.discard()

const clearInstallProgressListener = () => {
  if (installProgressUnlisten) {
    installProgressUnlisten()
    installProgressUnlisten = null
  }
}

const clearInstallWaitTimer = () => {
  if (installWaitTimer) {
    clearInterval(installWaitTimer)
    installWaitTimer = null
  }
}

const markInstallProgressSeen = () => {
  installLastEventAt.value = Date.now()
  installWaitHint.value = ''
}

const startInstallWaitTimer = () => {
  clearInstallWaitTimer()
  markInstallProgressSeen()
  installWaitTimer = setInterval(() => {
    if (!isInstalling.value || installStage.value === 'failed') return
    const elapsedMs = Date.now() - installLastEventAt.value
    if (elapsedMs >= 60000) {
      installWaitHint.value = t.value.updateDialog.installLongWaitHint
    } else if (elapsedMs >= 15000) {
      installWaitHint.value = t.value.updateDialog.installWaitHint
    } else {
      installWaitHint.value = ''
    }
  }, 1000)
}

const resetInstallProgress = () => {
  installStage.value = 'idle'
  installDetail.value = ''
  installWaitHint.value = ''
  installLastEventAt.value = 0
  clearInstallWaitTimer()
}

const getTauriApis = async () => {
  const [{ invoke }, { listen }] = await Promise.all([import('@tauri-apps/api/core'), import('@tauri-apps/api/event')])
  return { invoke, listen }
}

// The update info carries no channel; the metadata the updater read says which channel the version belongs to.
// Both hosts are asked at once, the answer of the mirror is reported when the main host did not answer.
let metadataId = 0
const resolveChannel = async () => {
  const id = ++metadataId
  channel.value = ''
  mirrorHost.value = ''
  const version = props.updateInfo?.version
  if (!version || isLatest.value) return
  const result = await fetchReleaseMetadata()
  if (id !== metadataId || !result.ok) return
  channel.value = channelOfVersion(result.metadata, version)
  mirrorHost.value = result.fallback ? result.host : ''
}

// 打开对话框时检查 temp 目录是否已有完整下载的安装包（上次取消安装或关闭应用时遗留）
let cacheCheckId = 0
const checkCachedInstaller = async () => {
  if (isLatest.value || !props.updateInfo?.download_name) return
  const checkId = ++cacheCheckId
  const filename = props.updateInfo.download_name
  try {
    const { invoke } = await getTauriApis()
    const cached = await invoke('check_cached_update', {
      filename,
      expectedSize: props.updateInfo.download_size || null,
      expectedSha256: props.updateInfo.download_sha256 || null,
      assetType: props.updateInfo.download_type || null,
      targetVersion: props.updateInfo.version || null,
    })
    if (
      cached &&
      checkId === cacheCheckId &&
      visible.value &&
      props.updateInfo?.download_name === filename &&
      !isDownloading.value &&
      !isInstalling.value
    ) {
      updateDownload.adoptCached(cached, props.updateInfo)
    }
  } catch (error) {
    console.warn('Failed to check cached installer:', error)
  }
}

const downloadErrorMessage = (error) => {
  const code = error && typeof error === 'object' ? error.code : ''
  console.error('Download update failed', error)
  const dialog = t.value.updateDialog
  const codes = Object.fromEntries(Object.entries(DOWNLOAD_ERROR_MESSAGE_KEYS).map(([key, name]) => [key, dialog[name]]))
  codes.default = dialog.downloadFailed
  const version = props.updateInfo?.version || ''
  return downloadFailureText(
    { code, skipped: error?.skipped ?? updateDownload.state.skipped, version },
    { codes: Object.fromEntries(Object.entries(codes).map(([key, text]) => [key, String(text || '').replaceAll('{version}', displayRelease(version))])), skip: dialog.sourceSkip },
  )
}

const handleDownloadFromGithub = () => {
  onlySource.value = 'github'
  handleDownload()
}

const handleDownload = async () => {
  const downloadUrl = props.updateInfo?.download_url
  if (!downloadUrl) {
    ElMessage.warning(t.value.updateDialog.noDownloadUrl)
    return
  }

  resetDownloadState()
  errorText.value = ''

  try {
    const filename = props.updateInfo.download_name || `sunshine-update-${props.updateInfo.version}.exe`
    // the host that is tried first follows the interface language and the source of the update settings; the other is the one tried when it fails
    const githubOnly = onlySource.value === 'github' ? githubUrlOf(props.updateInfo) : ''
    const urls = githubOnly
      ? { url: githubOnly, fallbackUrl: null }
      : orderedDownloadUrls({ url: downloadUrl, fallbackUrl: props.updateInfo.download_fallback_url || '' }, { source: updateSource.source.value, hasCdk: updateSource.hasCdk.value, locale: locale.value })
    const result = await updateDownload.start({
      url: urls.url,
      fallbackUrl: urls.fallbackUrl,
      filename,
      expectedSize: props.updateInfo.download_size || null,
      expectedSha256: props.updateInfo.download_sha256 || null,
      assetType: props.updateInfo.download_type || null,
      // the sources are checked against this release: one that lists another release is left out, not mixed in
      targetVersion: props.updateInfo.version || null,
      onlySource: githubOnly ? 'github' : null,
    })

    if (!result.ok) {
      console.error('Download update returned an unsuccessful result', result)
      errorStep.value = 'download'
      errorText.value = downloadErrorMessage({ code: result.errorCode, skipped: updateDownload.state.skipped })
    }
  } catch (error) {
    errorStep.value = 'download'
    errorText.value = downloadErrorMessage(error)
  }
}

const handleInstall = async (filePath) => {
  try {
    await ElMessageBox.confirm(
      t.value.updateDialog.installConfirm,
      t.value.updateDialog.installTitle,
      { confirmButtonText: t.value.updateDialog.installConfirmBtn, cancelButtonText: t.value.updateDialog.cancelBtn, type: 'warning' }
    )

    errorText.value = ''
    isInstalling.value = true
    resetInstallProgress()
    installStage.value = 'downloaded'
    startInstallWaitTimer()

    const { invoke, listen } = await getTauriApis()
    clearInstallProgressListener()
    installProgressUnlisten = await listen('install-progress', (event) => {
      const payload = event.payload || {}
      if (payload.stage) installStage.value = payload.stage
      installDetail.value = payload.detail || ''
      markInstallProgressSeen()
      if (payload.terminal) clearInstallWaitTimer()
    })

    await invoke('install_update', {
      filePath,
      targetVersion: props.updateInfo?.version || null,
      expectedSize: props.updateInfo?.download_size || null,
      expectedSha256: props.updateInfo?.download_sha256 || null,
      darkTheme: document.documentElement?.getAttribute('data-bs-theme') === 'dark',
    })

    ElMessage.success(t.value.updateDialog.installStarted)
  } catch (error) {
    if (error !== 'cancel') {
      clearInstallWaitTimer()
      clearInstallProgressListener()
      isInstalling.value = false
      if (isInstallerMismatch(error)) {
        // the file was the installer of another build (the native side has deleted it): the install is not offered again, the download is
        console.error('The downloaded installer is not the one of this update', error)
        updateDownload.discard()
        errorStep.value = 'download'
        errorText.value = t.value.updateDialog.installerMismatch.replaceAll('{version}', displayRelease(props.updateInfo?.version || ''))
        return
      }
      errorStep.value = 'install'
      errorText.value = t.value.updateDialog.installError.replace('{error}', error)
    }
  }
}

// Retry plans the sources again: a finished installer is installed, anything else is downloaded again, and the native side puts a source whose file failed behind the others
const handleRetry = () => {
  errorText.value = ''
  if (downloadedFilePath.value) handleInstall(downloadedFilePath.value)
  else handleDownload()
}

const handleOpenBrowser = async (url) => {
  // the release page, or the address a link of the sheet asks for (the full changelog)
  const releasePage = typeof url === 'string' ? url : sheet.value?.releasePage
  if (!releasePage) {
    ElMessage.warning(t.value.updateDialog.noReleasePage)
    return
  }

  try {
    const { invoke } = await getTauriApis()
    await invoke('open_external_url', { url: releasePage })
  } catch (error) {
    ElMessage.error(t.value.updateDialog.openBrowserError.replace('{error}', error))
  }
}

const handleCancel = () => {
  visible.value = false
  emit('close')
}

const handleSkipVersion = () => {
  const version = props.updateInfo?.version
  if (version) {
    emit('skip-version', version)
  }
  visible.value = false
  emit('close')
}

const resetState = () => {
  // a file, a failure or a note of an earlier release is not shown for this one
  updateDownload.reconcile(props.updateInfo)
  // An update download outlives this sheet: reopening it must show the
  // transfer, not erase it.
  if (!isDownloading.value && !downloadedFilePath.value) resetDownloadState()
  isInstalling.value = false
  errorText.value = updateDownload.state.errorCode
    ? downloadErrorMessage({ code: updateDownload.state.errorCode })
    : ''
  resetInstallProgress()
  clearInstallProgressListener()
}

onBeforeUnmount(() => {
  metadataId += 1
  clearInstallWaitTimer()
  clearInstallProgressListener()
})

// the update the sheet offers can change while it is open (a later check finds a newer release): what belongs to the earlier one goes
watch(
  () => [props.updateInfo?.version, props.updateInfo?.download_sha256],
  () => {
    if (updateDownload.reconcile(props.updateInfo)) errorText.value = ''
    if (props.modelValue) checkCachedInstaller()
  },
)

watch(
  () => props.modelValue,
  (newVal) => {
    if (newVal) {
      onlySource.value = ''
      resetState()
      updateDownload.refresh(props.updateInfo)
      checkCachedInstaller()
      resolveChannel()
    }
  },
  { immediate: true },
)
</script>

<style scoped>
.m-update-github { display: flex; justify-content: flex-start; margin-block: 0 4px; }
</style>
