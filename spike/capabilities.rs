//! 插件能力表：每種能力用什麼運行環境、歸屬到哪、輸出落在哪裡。
//!
//! 新開放一種能力，就在 [`CAPABILITIES`] 里加一行，再在宿主裡接上它的輸出位置。
//! 歸屬（owner）只決定插件設置放在哪個設置頁，以及哪個模塊關掉時整個插件暫停；
//! 一個腳本插件可以同時聲明幾種輸出，各輸出只受自己所在模塊的開關影響。

/// 腳本插件的輸出落在哪裡。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Surface {
    /// 待辦與置物裡的信息卡片，讀 `card`。
    TasksCard,
    /// 日曆當天的標註，讀 `marker`；舊版日程歸屬的 `card` 也落在這裡。
    CalendarMarker,
    /// AI 額度面板的一個來源，讀 `quota`。
    Quota,
    /// 頁面管理裡的獨立狀態頁，讀 `status`。
    StatusPage,
    /// 收起時由宿主輪播的短內容，讀 `pinned`。
    IslandPinned,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Role {
    /// 腳本插件的一種輸出。
    Output(Surface),
    /// 在同一個輸出位置擴展數據格式。
    CanvasOutput,
    /// 腳本插件的輔助能力：設置、定時刷新、私有存儲。
    Helper,
    /// 獨佔一個包的能力，包裡除了設置不再有其他能力，歸屬固定。
    Exclusive { owner: &'static str },
}

pub(super) struct Capability {
    pub name: &'static str,
    pub runtime: &'static str,
    pub role: Role,
}

pub(super) const CAPABILITIES: &[Capability] = &[
    Capability { name: "card", runtime: "js", role: Role::Output(Surface::TasksCard) },
    Capability { name: "calendar.marker", runtime: "js", role: Role::Output(Surface::CalendarMarker) },
    Capability { name: "quota.source", runtime: "js", role: Role::Output(Surface::Quota) },
    Capability { name: "status.page", runtime: "js", role: Role::Output(Surface::StatusPage) },
    Capability { name: "status.canvas", runtime: "js", role: Role::CanvasOutput },
    Capability { name: "island.pinned", runtime: "js", role: Role::Output(Surface::IslandPinned) },
    Capability { name: "settings", runtime: "*", role: Role::Helper },
    Capability { name: "timer", runtime: "js", role: Role::Helper },
    Capability { name: "storage", runtime: "js", role: Role::Helper },
    Capability { name: "action", runtime: "js", role: Role::Helper },
    Capability { name: "event", runtime: "js", role: Role::Helper },
    Capability { name: "notice", runtime: "js", role: Role::Helper },
    Capability {
        name: "appearance.material",
        runtime: "material",
        role: Role::Exclusive { owner: "appearance" },
    },
    Capability { name: "appearance.skin", runtime: "assets", role: Role::Exclusive { owner: "appearance" } },
    Capability { name: "appearance.icon", runtime: "assets", role: Role::Exclusive { owner: "appearance" } },
    Capability { name: "appearance.pet", runtime: "assets", role: Role::Exclusive { owner: "appearance" } },
    Capability { name: "appearance.font", runtime: "assets", role: Role::Exclusive { owner: "appearance" } },
    Capability {
        name: "navigation.shortcuts",
        runtime: "assets",
        role: Role::Exclusive { owner: "pages" },
    },
    Capability {
        name: "wireless.receivers",
        runtime: "assets",
        role: Role::Exclusive { owner: "wireless" },
    },
];

/// 腳本插件可以選的歸屬，以及這個歸屬至少要帶哪種輸出（設置放在那裡，總得在那裡看得到東西）。
pub(super) const SCRIPT_OWNERS: &[(&str, &[Surface])] = &[
    ("tasks", &[Surface::TasksCard]),
    ("calendar", &[Surface::CalendarMarker]),
    ("quota", &[Surface::Quota]),
    ("status", &[Surface::StatusPage]),
];

pub(super) fn find(name: &str) -> Option<&'static Capability> {
    CAPABILITIES.iter().find(|capability| capability.name == name)
}

/// 某個能力在這個歸屬下實際落在哪裡。舊版「日程歸屬 + card」是日曆標註。
pub(super) fn surface_of(name: &str, owner: &str) -> Option<Surface> {
    match find(name)?.role {
        Role::Output(Surface::TasksCard) if owner == "calendar" => Some(Surface::CalendarMarker),
        Role::Output(surface) => Some(surface),
        Role::CanvasOutput => Some(Surface::StatusPage),
        _ => None,
    }
}

/// 核對一份清單的能力組合。只看能力、運行環境和歸屬；各類包自己的細則由調用方另查。
pub(super) fn check(runtime: &str, owner: &str, names: &[String]) -> Result<Vec<Surface>, String> {
    let mut seen = std::collections::BTreeSet::new();
    let mut surfaces = Vec::new();
    let mut exclusive = None;
    for name in names {
        let Some(capability) = find(name) else {
            return Err(format!("當前版本不支持能力「{name}」"));
        };
        if !seen.insert(name.as_str()) {
            return Err(format!("能力「{name}」重複聲明"));
        }
        if capability.runtime != "*" && capability.runtime != runtime {
            return Err(format!("能力「{name}」需要 {} 運行環境", capability.runtime));
        }
        match capability.role {
            Role::Exclusive { owner: required } => {
                if owner != required {
                    return Err(format!("能力「{name}」的歸屬須為 {required}"));
                }
                exclusive = Some(name.as_str());
            }
            Role::Output(_) => {
                let surface = surface_of(name, owner).unwrap();
                if surfaces.contains(&surface) {
                    return Err("同一個輸出位置只能聲明一次，日程歸屬的 card 已經是日曆標註".into());
                }
                surfaces.push(surface);
            }
            Role::CanvasOutput | Role::Helper => {}
        }
    }
    if seen.contains("action") && !seen.contains("status.page") {
        return Err("按鈕和表單需要同時聲明 status.page".into());
    }
    if seen.contains("status.canvas") && !seen.contains("status.page") {
        return Err("自繪畫布需要同時聲明 status.page".into());
    }
    if let Some(name) = exclusive {
        if names.iter().any(|other| other != name && other != "settings") {
            return Err(format!("能力「{name}」只能和 settings 一起聲明"));
        }
        return Ok(surfaces);
    }
    if runtime != "js" {
        return Err(format!("{runtime} 插件需要聲明對應的專屬能力"));
    }
    let Some((_, home)) = SCRIPT_OWNERS.iter().find(|(key, _)| *key == owner) else {
        return Err(format!("腳本插件的歸屬須為 tasks、calendar、quota 或 status，當前為 {owner}"));
    };
    if !home.iter().any(|surface| surfaces.contains(surface)) {
        return Err(match owner {
            "quota" => "AI 額度歸屬的插件需要聲明 quota.source".into(),
            "status" => "狀態頁歸屬的插件需要聲明 status.page".into(),
            "calendar" => "日程歸屬的插件需要聲明 card 或 calendar.marker".into(),
            _ => "待辦歸屬的插件需要聲明 card".into(),
        });
    }
    Ok(surfaces)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|name| (*name).to_owned()).collect()
    }

    #[test]
    fn table_accepts_every_shipped_combination() {
        for (runtime, owner, list) in [
            ("js", "tasks", &["card", "settings", "timer", "storage"][..]),
            ("js", "calendar", &["card", "settings"]),
            ("js", "quota", &["quota.source", "settings", "timer"]),
            ("js", "status", &["status.page", "timer", "storage"]),
            ("js", "status", &["status.page", "status.canvas"]),
            ("material", "appearance", &["appearance.material", "settings"]),
            ("assets", "pages", &["navigation.shortcuts"]),
            ("assets", "wireless", &["wireless.receivers"]),
        ] {
            assert!(check(runtime, owner, &names(list)).is_ok(), "{owner} {list:?}");
        }
    }

    #[test]
    fn one_script_can_fill_several_surfaces() {
        assert_eq!(
            check("js", "tasks", &names(&["card", "calendar.marker", "timer"])).unwrap(),
            [Surface::TasksCard, Surface::CalendarMarker]
        );
        assert_eq!(
            check("js", "status", &names(&["status.page", "card"])).unwrap(),
            [Surface::StatusPage, Surface::TasksCard]
        );
        assert_eq!(
            check("js", "calendar", &names(&["calendar.marker"])).unwrap(),
            [Surface::CalendarMarker]
        );
    }

    #[test]
    fn table_rejects_unknown_duplicate_and_mismatched_capabilities() {
        for (runtime, owner, list) in [
            ("js", "tasks", &["network", "card"][..]),
            ("js", "status", &["status.canvas"]),
            ("js", "tasks", &["card", "card"]),
            ("js", "calendar", &["card", "calendar.marker"]),
            ("js", "tasks", &["calendar.marker"]),
            ("js", "quota", &["card"]),
            ("js", "appearance", &["card"]),
            ("js", "tasks", &["card", "appearance.material"]),
            ("material", "appearance", &["appearance.material", "timer"]),
            ("material", "tasks", &["appearance.material"]),
            ("assets", "pages", &["navigation.shortcuts", "settings", "card"]),
            ("js", "tasks", &["settings"]),
        ] {
            assert!(check(runtime, owner, &names(list)).is_err(), "{owner} {list:?}");
        }
    }
}
