//! The texts of the pairing-request notification in the 20 languages of the panel.
//!
//! The rest of the tray (its menu, the other notifications) speaks Chinese, Japanese and English (`TrayStrings`); the notification that asks for a PIN is the one a person meets
//! before anything else is on screen, so it is in the language of the host as the panel knows it: the Sunshine `locale` setting names one of the 20 codes of the panel
//! (`desktop/i18n/merge.js` DESKTOP_LOCALES, a test keeps the two lists equal). The tray menu itself stays in its three languages, so the menu item the second text points
//! to is named as the menu shows it: 「完成配对」 and 「ペアリングを完了」, and `Complete Pairing` everywhere else.

pub(in crate::tray) struct PairingTexts {
    pub incoming_pairing: &'static str,
    /// Where a click opens the PIN entry (Windows).
    pub pairing_instruction: &'static str,
    /// Where it does not: the tray menu item leads there.
    pub pairing_menu_instruction: &'static str,
}

pub(in crate::tray) const CODES: [&str; 20] = [
    "en", "zh", "zh_TW", "en_GB", "en_US", "de", "fr", "es", "it", "ja", "ko", "ru", "pt_BR", "pt", "bg", "cs", "pl", "sv", "tr", "uk",
];

/// One of the 20 codes for a locale name as Sunshine or the system writes it (`de`, `pt-BR`, `zh_TW`, `en_GB.UTF-8`); `None` for a language the panel does not have.
pub(in crate::tray) fn code_for(locale: &str) -> Option<&'static str> {
    let lowered = locale.trim().to_ascii_lowercase();
    let name = lowered.split(['.', '@']).next().unwrap_or("").replace('-', "_");
    let primary = name.split('_').next().unwrap_or("");
    match name.as_str() {
        "zh_tw" | "zh_hk" | "zh_mo" | "zh_hant" => return Some("zh_TW"),
        "pt_br" => return Some("pt_BR"),
        "en_gb" => return Some("en_GB"),
        "en_us" => return Some("en_US"),
        _ => {}
    }
    CODES.iter().copied().find(|code| code.eq_ignore_ascii_case(primary) && !code.contains('_'))
}

pub(in crate::tray) fn texts(code: &str) -> &'static PairingTexts {
    match code {
        "zh" => &ZH,
        "zh_TW" => &ZH_TW,
        "ja" => &JA,
        "de" => &DE,
        "fr" => &FR,
        "es" => &ES,
        "it" => &IT,
        "ko" => &KO,
        "ru" => &RU,
        "pt_BR" => &PT_BR,
        "pt" => &PT,
        "bg" => &BG,
        "cs" => &CS,
        "pl" => &PL,
        "sv" => &SV,
        "tr" => &TR,
        "uk" => &UK,
        // en, en_GB, en_US, and a language the panel does not have
        _ => &EN,
    }
}

const EN: PairingTexts = PairingTexts {
    incoming_pairing: "Moonlight pairing request",
    pairing_instruction: "Click this notification to enter the 4-digit PIN shown in Moonlight.",
    pairing_menu_instruction: "Open Complete Pairing from the Sunshine tray menu and enter the 4-digit PIN shown in Moonlight.",
};
const ZH: PairingTexts = PairingTexts {
    incoming_pairing: "Moonlight 配对请求",
    pairing_instruction: "点击此通知，输入 Moonlight 上显示的 4 位 PIN。",
    pairing_menu_instruction: "打开 Sunshine 托盘菜单中的“完成配对”，输入 Moonlight 上显示的 4 位 PIN。",
};
const ZH_TW: PairingTexts = PairingTexts {
    incoming_pairing: "Moonlight 配對請求",
    pairing_instruction: "點擊此通知，輸入 Moonlight 上顯示的 4 位 PIN。",
    // the tray menu is in Chinese, with the simplified label
    pairing_menu_instruction: "開啟 Sunshine 系統匣選單中的“完成配对”，輸入 Moonlight 上顯示的 4 位 PIN。",
};
const JA: PairingTexts = PairingTexts {
    incoming_pairing: "Moonlight ペアリング要求",
    pairing_instruction: "この通知をクリックし、Moonlight に表示された 4 桁の PIN を入力してください。",
    pairing_menu_instruction: "Sunshine のトレイメニューから「ペアリングを完了」を開き、Moonlight に表示された 4 桁の PIN を入力してください。",
};
const DE: PairingTexts = PairingTexts {
    incoming_pairing: "Moonlight-Kopplungsanfrage",
    pairing_instruction: "Auf diese Benachrichtigung klicken und die in Moonlight angezeigte 4-stellige PIN eingeben.",
    pairing_menu_instruction: "Im Sunshine-Tray-Menü „Complete Pairing“ öffnen und die in Moonlight angezeigte 4-stellige PIN eingeben.",
};
const FR: PairingTexts = PairingTexts {
    incoming_pairing: "Demande de couplage Moonlight",
    pairing_instruction: "Cliquez sur cette notification pour saisir le code PIN à 4 chiffres affiché dans Moonlight.",
    pairing_menu_instruction: "Ouvrez « Complete Pairing » dans le menu de la zone de notification Sunshine, puis saisissez le code PIN à 4 chiffres affiché dans Moonlight.",
};
const ES: PairingTexts = PairingTexts {
    incoming_pairing: "Solicitud de emparejamiento de Moonlight",
    pairing_instruction: "Haga clic en esta notificación para introducir el PIN de 4 dígitos que muestra Moonlight.",
    pairing_menu_instruction: "Abra «Complete Pairing» en el menú de la bandeja de Sunshine e introduzca el PIN de 4 dígitos que muestra Moonlight.",
};
const IT: PairingTexts = PairingTexts {
    incoming_pairing: "Richiesta di abbinamento Moonlight",
    pairing_instruction: "Fai clic su questa notifica per inserire il PIN a 4 cifre mostrato in Moonlight.",
    pairing_menu_instruction: "Apri «Complete Pairing» dal menu nell’area di notifica di Sunshine e inserisci il PIN a 4 cifre mostrato in Moonlight.",
};
const KO: PairingTexts = PairingTexts {
    incoming_pairing: "Moonlight 페어링 요청",
    pairing_instruction: "이 알림을 클릭한 후 Moonlight에 표시된 4자리 PIN을 입력하세요.",
    pairing_menu_instruction: "Sunshine 트레이 메뉴에서 ‘Complete Pairing’을 열고 Moonlight에 표시된 4자리 PIN을 입력하세요.",
};
const RU: PairingTexts = PairingTexts {
    incoming_pairing: "Запрос на сопряжение Moonlight",
    pairing_instruction: "Нажмите на это уведомление и введите 4-значный PIN-код, показанный в Moonlight.",
    pairing_menu_instruction: "Откройте «Complete Pairing» в меню Sunshine в области уведомлений и введите 4-значный PIN-код, показанный в Moonlight.",
};
const PT_BR: PairingTexts = PairingTexts {
    incoming_pairing: "Solicitação de emparelhamento do Moonlight",
    pairing_instruction: "Clique nesta notificação para digitar o PIN de 4 dígitos exibido no Moonlight.",
    pairing_menu_instruction: "Abra “Complete Pairing” no menu da bandeja do Sunshine e digite o PIN de 4 dígitos exibido no Moonlight.",
};
const PT: PairingTexts = PairingTexts {
    incoming_pairing: "Pedido de emparelhamento do Moonlight",
    pairing_instruction: "Clique nesta notificação para introduzir o PIN de 4 dígitos apresentado no Moonlight.",
    pairing_menu_instruction: "Abra “Complete Pairing” no menu da área de notificação do Sunshine e introduza o PIN de 4 dígitos apresentado no Moonlight.",
};
const BG: PairingTexts = PairingTexts {
    incoming_pairing: "Заявка за сдвояване от Moonlight",
    pairing_instruction: "Щракнете върху това известие и въведете 4-цифрения PIN, показан в Moonlight.",
    pairing_menu_instruction: "Отворете „Complete Pairing“ от менюто на Sunshine в областта за известия и въведете 4-цифрения PIN, показан в Moonlight.",
};
const CS: PairingTexts = PairingTexts {
    incoming_pairing: "Žádost o spárování z Moonlightu",
    pairing_instruction: "Klikněte na toto oznámení a zadejte čtyřmístný PIN zobrazený v Moonlightu.",
    pairing_menu_instruction: "Otevřete v nabídce Sunshine v oznamovací oblasti položku „Complete Pairing“ a zadejte čtyřmístný PIN zobrazený v Moonlightu.",
};
const PL: PairingTexts = PairingTexts {
    incoming_pairing: "Prośba o sparowanie z Moonlight",
    pairing_instruction: "Kliknij to powiadomienie, aby wpisać 4-cyfrowy kod PIN wyświetlony w Moonlight.",
    pairing_menu_instruction: "Otwórz „Complete Pairing” w menu Sunshine w obszarze powiadomień i wpisz 4-cyfrowy kod PIN wyświetlony w Moonlight.",
};
const SV: PairingTexts = PairingTexts {
    incoming_pairing: "Parkopplingsförfrågan från Moonlight",
    pairing_instruction: "Klicka på aviseringen och ange den fyrsiffriga PIN-koden som visas i Moonlight.",
    pairing_menu_instruction: "Öppna ”Complete Pairing” i Sunshines meny i aktivitetsfältet och ange den fyrsiffriga PIN-koden som visas i Moonlight.",
};
const TR: PairingTexts = PairingTexts {
    incoming_pairing: "Moonlight eşleştirme isteği",
    pairing_instruction: "Moonlight’ta görünen 4 haneli PIN’i girmek için bu bildirime tıklayın.",
    pairing_menu_instruction: "Sunshine bildirim alanı menüsünden “Complete Pairing” öğesini açın ve Moonlight’ta görünen 4 haneli PIN’i girin.",
};
const UK: PairingTexts = PairingTexts {
    incoming_pairing: "Запит на спарювання від Moonlight",
    pairing_instruction: "Натисніть це сповіщення й введіть 4-значний PIN-код, показаний у Moonlight.",
    pairing_menu_instruction: "Відкрийте «Complete Pairing» у меню Sunshine в області сповіщень і введіть 4-значний PIN-код, показаний у Moonlight.",
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_code_of_the_panel_has_its_own_texts_except_the_english_variants() {
        for code in CODES {
            let t = texts(code);
            for text in [t.incoming_pairing, t.pairing_instruction, t.pairing_menu_instruction] {
                assert!(!text.trim().is_empty(), "{code}");
            }
            if !matches!(code, "en" | "en_GB" | "en_US") {
                assert_ne!(t.incoming_pairing, EN.incoming_pairing, "{code} is translated");
                assert_ne!(t.pairing_instruction, EN.pairing_instruction, "{code} is translated");
                assert_ne!(t.pairing_menu_instruction, EN.pairing_menu_instruction, "{code} is translated");
            }
        }
    }

    #[test]
    fn the_codes_are_the_languages_of_the_panel() {
        let merge = include_str!("../../../../src/renderer/desktop/i18n/merge.js");
        let line = merge.lines().find(|line| line.starts_with("export const DESKTOP_LOCALES")).expect("the list of languages");
        let listed: Vec<&str> = line.split('\'').skip(1).step_by(2).collect();
        assert_eq!(listed, CODES, "src-tauri/src/tray/notifications/pairing_texts.rs CODES and desktop/i18n/merge.js DESKTOP_LOCALES list the same codes in the same order");
    }

    #[test]
    fn the_locale_names_sunshine_and_the_system_write_become_codes() {
        for (name, code) in [
            ("en", "en"), ("zh", "zh"), ("zh-CN", "zh"), ("zh_TW", "zh_TW"), ("zh-Hant", "zh_TW"), ("zh-HK", "zh_TW"), ("de", "de"), ("de_DE.UTF-8", "de"), ("pt-BR", "pt_BR"),
            ("pt_PT", "pt"), ("en_GB.UTF-8", "en_GB"), ("en-US", "en_US"), ("en_AU", "en"), ("ja_JP", "ja"), ("UK", "uk"), ("  sv  ", "sv"),
        ] {
            assert_eq!(code_for(name), Some(code), "{name}");
        }
        for name in ["", "xx", "C", "POSIX", "he", "fil"] {
            assert_eq!(code_for(name), None, "{name:?}");
        }
        assert!(texts("not-a-code").incoming_pairing.starts_with("Moonlight pairing"));
    }

    #[test]
    fn the_three_languages_of_the_tray_keep_the_words_they_always_had() {
        assert_eq!(texts("en").pairing_instruction, "Click this notification to enter the 4-digit PIN shown in Moonlight.");
        assert_eq!(texts("zh").incoming_pairing, "Moonlight 配对请求");
        assert_eq!(texts("ja").pairing_menu_instruction, "Sunshine のトレイメニューから「ペアリングを完了」を開き、Moonlight に表示された 4 桁の PIN を入力してください。");
    }
}
