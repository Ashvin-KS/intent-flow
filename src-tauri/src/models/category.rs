use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Category {
    pub id: i32,
    pub name: String,
    pub icon: String,
    pub color: String,
    pub keywords: Vec<String>,
    pub apps: Vec<String>,
}

impl Default for Category {
    fn default() -> Self {
        Self {
            id: 0,
            name: String::new(),
            icon: String::from("folder"),
            color: String::from("#6b7280"),
            keywords: vec![],
            apps: vec![],
        }
    }
}

pub fn get_default_categories() -> Vec<Category> {
    vec![
        Category {
            id: 1,
            name: "Development".to_string(),
            icon: "code".to_string(),
            color: "#3b82f6".to_string(),
            keywords: vec!["vscode".to_string(), "terminal".to_string(), "git".to_string(), "code".to_string()],
            apps: vec!["Code.exe".to_string(), "git-bash.exe".to_string()],
        },
        Category {
            id: 2,
            name: "Browser".to_string(),
            icon: "globe".to_string(),
            color: "#10b981".to_string(),
            keywords: vec!["chrome".to_string(), "firefox".to_string(), "edge".to_string()],
            apps: vec!["chrome.exe".to_string(), "firefox.exe".to_string(), "msedge.exe".to_string()],
        },
        Category {
            id: 3,
            name: "Communication".to_string(),
            icon: "message-circle".to_string(),
            color: "#8b5cf6".to_string(),
            keywords: vec!["slack".to_string(), "discord".to_string(), "teams".to_string()],
            apps: vec!["slack.exe".to_string(), "discord.exe".to_string()],
        },
        Category {
            id: 4,
            name: "Entertainment".to_string(),
            icon: "play".to_string(),
            color: "#f59e0b".to_string(),
            keywords: vec!["youtube".to_string(), "spotify".to_string(), "netflix".to_string()],
            apps: vec!["spotify.exe".to_string()],
        },
        Category {
            id: 5,
            name: "Productivity".to_string(),
            icon: "check-square".to_string(),
            color: "#ec4899".to_string(),
            keywords: vec!["notion".to_string(), "obsidian".to_string(), "todo".to_string()],
            apps: vec!["notion.exe".to_string(), "obsidian.exe".to_string()],
        },
        Category {
            id: 6,
            name: "System".to_string(),
            icon: "settings".to_string(),
            color: "#6b7280".to_string(),
            keywords: vec!["explorer".to_string(), "settings".to_string()],
            apps: vec!["explorer.exe".to_string()],
        },
        Category {
            id: 7,
            name: "Other".to_string(),
            icon: "more-horizontal".to_string(),
            color: "#9ca3af".to_string(),
            keywords: vec![],
            apps: vec![],
        },
    ]
}
