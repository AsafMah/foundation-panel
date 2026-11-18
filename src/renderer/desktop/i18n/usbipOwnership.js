const en = {
  replaceLegacy: 'Clean up old USB/IP',
  replaceConfirm: 'This uninstalls the existing usbip-win2 and installs the slim Foundation Sunshine package.\nIts GUI and SDK are removed too; cancel if you still use them.\nAttached USB devices are detached.',
  externalReplaceable: 'The existing USB/IP version or driver is incompatible. Only 0.9.8.1 is supported.',
  replaceSuccess: 'Replaced with the slim package. USB/IP is ready.',
  errors: {
    'USBIP-REPLACE-001': 'This USB/IP installation cannot be replaced from here. Nothing was changed.',
    'USBIP-REPLACE-002': 'The replacement failed and was rolled back. The USB root hubs are healthy and USB/IP is not installed; install it again.',
    'USBIP-REPLACE-003': 'The replacement failed and the USB root hubs were not fully recovered. Restart the PC, then check your USB devices.',
    'USBIP-REPLACE-004': 'The replacement failed. Refresh to see the current state.',
    'USBIP-REPLACE-005': 'Detach the attached USB devices first.',
  },
  externallyManaged: 'Externally managed',
  externalReuse: 'Using your compatible USB/IP installation. You manage its installation, repair and removal.',
  externalDecision: 'Your USB/IP installation is preserved. Only 0.9.8.1 is supported. If its version or driver is incompatible, choose whether to replace or repair it in Windows Installed apps.',
}
const zh = {
  replaceLegacy: '清理旧版 USB/IP',
  replaceConfirm: '将卸载现有的 usbip-win2，换成 Foundation Sunshine 精简版。\n它的图形界面和 SDK 也会被移除；如果你还在用，请取消。\n已挂载的 USB 设备会断开。',
  externalReplaceable: '现有 USB/IP 的版本或驱动不兼容，仅支持 0.9.8.1。',
  replaceSuccess: '已换成精简版，USB/IP 就绪。',
  errors: {
    'USBIP-REPLACE-001': '这份 USB/IP 无法在此替换，没有做任何改动。',
    'USBIP-REPLACE-002': '替换失败并已回滚。USB 根集线器正常，USB/IP 当前未安装，请重新安装。',
    'USBIP-REPLACE-003': '替换失败，USB 根集线器没有完全恢复。请重启电脑后检查 USB 设备。',
    'USBIP-REPLACE-004': '替换失败，请刷新后查看当前状态。',
    'USBIP-REPLACE-005': '请先断开已挂载的 USB 设备。',
  },
  externallyManaged: '外部管理',
  externalReuse: '使用现有的兼容 USB/IP 安装；安装、修复和卸载均由您管理。',
  externalDecision: '现有 USB/IP 安装将保留。仅支持 0.9.8.1；版本或驱动不兼容时，请自行决定是否先在 Windows「已安装的应用」中替换或修复。',
}
const zh_TW = {
  replaceLegacy: '清理舊版 USB/IP',
  replaceConfirm: '將解除安裝現有的 usbip-win2，換成 Foundation Sunshine 精簡版。\n它的圖形介面和 SDK 也會被移除；如果你還在使用，請取消。\n已掛載的 USB 裝置會中斷連線。',
  externalReplaceable: '現有 USB/IP 的版本或驅動程式不相容，僅支援 0.9.8.1。',
  replaceSuccess: '已換成精簡版，USB/IP 就緒。',
  errors: {
    'USBIP-REPLACE-001': '這份 USB/IP 無法在此替換，沒有做任何變更。',
    'USBIP-REPLACE-002': '替換失敗並已還原。USB 根集線器正常，USB/IP 目前未安裝，請重新安裝。',
    'USBIP-REPLACE-003': '替換失敗，USB 根集線器沒有完全恢復。請重新啟動電腦後檢查 USB 裝置。',
    'USBIP-REPLACE-004': '替換失敗，請重新整理後查看目前狀態。',
    'USBIP-REPLACE-005': '請先中斷已掛載的 USB 裝置。',
  },
  externallyManaged: '外部管理',
  externalReuse: '使用現有的相容 USB/IP 安裝；安裝、修復和解除安裝均由您管理。',
  externalDecision: '現有 USB/IP 安裝將保留。僅支援 0.9.8.1；版本或驅動程式不相容時，請自行決定是否先在 Windows「已安裝的應用程式」中替換或修復。',
}

/** Extend the small capability vocabulary without growing the legacy locale tables. */
export function withUsbipOwnership(table, locale) {
  const messages = locale === 'zh' ? zh : locale === 'zh_TW' ? zh_TW : en
  return {
    ...table,
    deviceHub: { ...table.deviceHub, usb: { ...table.deviceHub?.usb, ...messages, errors: { ...table.deviceHub?.usb?.errors, ...messages.errors } } },
  }
}
