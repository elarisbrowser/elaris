// SPDX-License-Identifier: GPL-3.0-only

//! The engine's processes as Ladybird's Task Manager lists them: the browser's own processes first,
//! then one group per site and privacy, then the spare processes kept ready for the next site.

use serde::Deserialize;

/// One Ladybird process, as the bridge describes it.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Process {
    /// Ladybird's child ID, which, unlike the PID, never comes to name another process.
    pub id: u64,
    pub pid: i64,
    /// Ladybird's process name, such as `WebContent`.
    pub kind: String,
    #[serde(default)]
    pub private: bool,
    pub title: Option<String>,
    /// The site the process works for, or empty.
    #[serde(default)]
    pub site: String,
    /// The process this one works for, such as a page's media server.
    pub owner: Option<u64>,
    /// Percent of one core.
    pub cpu: f64,
    /// Bytes.
    pub memory: u64,
    /// Kept ready for the next site.
    #[serde(default)]
    pub spare: bool,
    #[serde(default)]
    pub tabs: Vec<ProcessTab>,
    #[serde(default)]
    pub frames: Vec<ProcessFrame>,
}

/// A tab whose page the process hosts.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct ProcessTab {
    /// The browser's tab ID, or 0 for a view the browser does not own.
    pub view: u64,
    pub title: String,
}

/// A frame of another process's page that this process hosts.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct ProcessFrame {
    pub name: String,
    pub url: String,
    pub sandboxed: bool,
}

impl Process {
    /// The browser itself, rather than a helper it started.
    pub fn is_browser(&self) -> bool {
        matches!(self.kind.as_str(), "Browser" | "UI" | "Hub")
    }

    /// Ending the browser's own process would end the browser.
    pub fn can_end(&self) -> bool {
        !self.is_browser()
    }

    /// The row's name. A process listed outside a site group carries its title and privacy, which
    /// its group otherwise shows.
    pub fn name(&self, grouped: bool) -> String {
        let mut name = self.kind.clone();
        if grouped {
            return name;
        }
        if let Some(title) = self.title.as_deref().filter(|_| self.kind != "WebContent") {
            name = format!("{name} — {title}");
        }
        if self.private {
            name = format!("{name} (Private)");
        }
        name
    }

    /// What the row says after its name: how many frames of other pages it hosts.
    pub fn note(&self) -> Option<String> {
        let frames = self.frames.len();
        let sandboxed = self.frames.iter().filter(|frame| frame.sandboxed).count();
        match (frames, sandboxed) {
            (0, _) => None,
            (1, 1) => Some("1 sandboxed frame".into()),
            (_, s) if s == frames => Some(format!("{frames} sandboxed frames")),
            (_, 0) => Some(plural(frames, "frame")),
            _ => Some(format!("{frames} frames, {sandboxed} sandboxed")),
        }
    }

    fn matches(&self, query: &str) -> bool {
        contains(&self.kind, query)
            || self
                .title
                .as_deref()
                .is_some_and(|title| contains(title, query))
            || contains(&self.site, query)
            || self.pid.to_string().contains(query)
            || self.tabs.iter().any(|tab| contains(&tab.title, query))
            || self
                .frames
                .iter()
                .any(|frame| contains(&frame.name, query) || contains(&frame.url, query))
    }
}

/// The processes that work for one site, in one privacy.
#[derive(Clone, Debug, PartialEq)]
pub struct Site {
    /// Stable across refreshes, so selection and expansion survive them.
    pub key: String,
    pub name: String,
    pub processes: Vec<Process>,
    pub cpu: f64,
    pub memory: u64,
}

impl Site {
    /// How many tabs show the site and how many of its frames other pages embed.
    pub fn note(&self) -> String {
        let tabs: usize = self.processes.iter().map(|p| p.tabs.len()).sum();
        let frames: usize = self.processes.iter().map(|p| p.frames.len()).sum();
        let mut note = Vec::new();
        if tabs > 0 {
            note.push(plural(tabs, "tab"));
        }
        if frames > 0 {
            note.push(plural(frames, "embedded frame"));
        }
        note.join(" · ")
    }
}

/// Every process, grouped and totalled.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Overview {
    /// The browser's own processes, the browser first.
    pub browser: Vec<Process>,
    pub sites: Vec<Site>,
    pub spare: Vec<Process>,
    pub cpu: f64,
    pub memory: u64,
    pub count: usize,
}

impl Overview {
    /// Groups the bridge's snapshot. Sites and the processes in each are listed by memory, largest first.
    pub fn new(processes: Vec<Process>) -> Self {
        let mut overview = Self {
            cpu: processes.iter().map(|p| p.cpu).sum(),
            memory: processes.iter().map(|p| p.memory).sum(),
            count: processes.len(),
            ..Self::default()
        };
        // A process listed with the one it works for, which may itself work for a third.
        let section = |process: &Process| -> Process {
            let mut section = process;
            for _ in 0..processes.len() {
                let Some(owner) = section.owner.filter(|_| section.site.is_empty()) else {
                    break;
                };
                let Some(found) = processes.iter().find(|p| p.id == owner) else {
                    break;
                };
                section = found;
            }
            section.clone()
        };
        for process in &processes {
            let section = section(process);
            if section.spare {
                overview.spare.push(process.clone());
                continue;
            }
            if section.site.is_empty() {
                overview.browser.push(process.clone());
                continue;
            }
            let key = format!(
                "{}:{}",
                if process.private {
                    "private"
                } else {
                    "regular"
                },
                section.site
            );
            let site = match overview.sites.iter_mut().position(|site| site.key == key) {
                Some(index) => &mut overview.sites[index],
                None => {
                    let name = if process.private {
                        format!("{} (Private)", section.site)
                    } else {
                        section.site.clone()
                    };
                    overview.sites.push(Site {
                        key,
                        name,
                        processes: Vec::new(),
                        cpu: 0.0,
                        memory: 0,
                    });
                    overview.sites.last_mut().expect("just pushed")
                }
            };
            site.cpu += process.cpu;
            site.memory += process.memory;
            site.processes.push(process.clone());
        }
        let by_memory = |a: &Process, b: &Process| b.memory.cmp(&a.memory);
        // The browser leads its own processes, with the UI ahead of the Hub.
        overview.browser.sort_by(|a, b| {
            let rank = |p: &Process| match p.kind.as_str() {
                "UI" => 0,
                "Browser" | "Hub" => 1,
                _ => 2,
            };
            rank(a).cmp(&rank(b)).then_with(|| by_memory(a, b))
        });
        for site in &mut overview.sites {
            site.processes.sort_by(by_memory);
        }
        overview
            .sites
            .sort_by_key(|site| std::cmp::Reverse(site.memory));
        overview.spare.sort_by(by_memory);
        overview
    }

    /// Keeps the processes that match `query`, ignoring case. A site whose name matches keeps all of
    /// its processes. Totals still describe every process.
    pub fn filtered(&self, query: &str) -> Self {
        let query = query.trim().to_lowercase();
        if query.is_empty() {
            return self.clone();
        }
        let keep = |processes: &[Process]| -> Vec<Process> {
            processes
                .iter()
                .filter(|p| p.matches(&query))
                .cloned()
                .collect()
        };
        let sites = self
            .sites
            .iter()
            .filter_map(|site| {
                if contains(&site.name, &query) {
                    return Some(site.clone());
                }
                let processes = keep(&site.processes);
                (!processes.is_empty()).then(|| Site {
                    processes,
                    ..site.clone()
                })
            })
            .collect();
        Self {
            browser: keep(&self.browser),
            sites,
            spare: keep(&self.spare),
            ..self.clone()
        }
    }

    pub fn is_empty(&self) -> bool {
        self.browser.is_empty() && self.sites.is_empty() && self.spare.is_empty()
    }

    /// The process with this child ID.
    pub fn process(&self, id: u64) -> Option<&Process> {
        self.browser
            .iter()
            .chain(self.sites.iter().flat_map(|site| &site.processes))
            .chain(&self.spare)
            .find(|process| process.id == id)
    }
}

/// Reads the bridge's JSON snapshot.
pub fn parse(json: &str) -> Result<Vec<Process>, String> {
    serde_json::from_str(json).map_err(|error| format!("invalid process snapshot: {error}"))
}

/// Memory as Ladybird's Task Manager shows it: binary units, one decimal.
pub fn format_memory(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["bytes", "KB", "MB", "GB", "TB"];
    if bytes < 1024 {
        return format!("{bytes} bytes");
    }
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

pub fn format_cpu(percent: f64) -> String {
    format!("{percent:.1}%")
}

pub fn plural(count: usize, noun: &str) -> String {
    match count {
        1 => format!("1 {noun}"),
        _ if noun.ends_with('s') => format!("{count} {noun}es"),
        _ => format!("{count} {noun}s"),
    }
}

fn contains(haystack: &str, lowercase_needle: &str) -> bool {
    haystack.to_lowercase().contains(lowercase_needle)
}

/// What one browser tab's page uses: its share of the processes that host it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TabUsage {
    /// Percent of one core.
    pub cpu: f64,
    /// Bytes.
    pub memory: u64,
}

/// What each browser tab's page uses, by the tab's view ID. A process shared by several tabs of
/// one site is split evenly between them, and the helpers it owns, such as its media server, count
/// towards it.
pub fn tab_usage(processes: &[Process]) -> std::collections::HashMap<u64, TabUsage> {
    let mut usage = std::collections::HashMap::<u64, TabUsage>::new();
    for process in processes.iter().filter(|process| !process.tabs.is_empty()) {
        let helpers = processes
            .iter()
            .filter(|helper| helper.owner == Some(process.id));
        let (cpu, memory) = helpers.fold((process.cpu, process.memory), |(cpu, memory), helper| {
            (cpu + helper.cpu, memory + helper.memory)
        });
        let tabs = process.tabs.len();
        for tab in process.tabs.iter().filter(|tab| tab.view != 0) {
            let entry = usage.entry(tab.view).or_default();
            entry.cpu += cpu / tabs as f64;
            entry.memory += memory / tabs as u64;
        }
    }
    usage
}

/// The memory each browser tab's page holds, in bytes, by the tab's view ID, as [`tab_usage`]
/// shares it out.
pub fn tab_memory(processes: &[Process]) -> std::collections::HashMap<u64, u64> {
    tab_usage(processes)
        .into_iter()
        .map(|(view, usage)| (view, usage.memory))
        .collect()
}

impl Overview {
    /// What each browser tab's page uses, by the tab's view ID.
    pub fn tab_usage(&self) -> std::collections::HashMap<u64, TabUsage> {
        let processes: Vec<Process> = self
            .browser
            .iter()
            .chain(self.sites.iter().flat_map(|site| &site.processes))
            .chain(&self.spare)
            .cloned()
            .collect();
        tab_usage(&processes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn process(id: u64, kind: &str, site: &str, memory: u64) -> Process {
        Process {
            id,
            pid: id as i64 + 100,
            kind: kind.into(),
            private: false,
            title: None,
            site: site.into(),
            owner: None,
            cpu: 1.0,
            memory,
            spare: false,
            tabs: Vec::new(),
            frames: Vec::new(),
        }
    }

    #[test]
    fn tab_memory_splits_shared_processes_and_counts_helpers() {
        let tab = |view| ProcessTab {
            view,
            title: String::new(),
        };
        let mut shared = process(2, "WebContent", "a.example", 600);
        shared.tabs = vec![tab(7), tab(8)];
        let mut alone = process(3, "WebContent", "b.example", 100);
        alone.tabs = vec![tab(9)];
        let mut helper = process(4, "MediaServer", "", 50);
        helper.owner = Some(3);
        let memory = tab_memory(&[process(1, "Browser", "", 999), shared, alone, helper]);
        assert_eq!(memory[&7], 300);
        assert_eq!(memory[&8], 300);
        assert_eq!(memory[&9], 150);
        assert_eq!(memory.len(), 3);
    }

    #[test]
    fn tab_usage_shares_cpu_as_memory_and_reads_from_an_overview() {
        let tab = |view| ProcessTab {
            view,
            title: String::new(),
        };
        let mut page = process(2, "WebContent", "a.example", 400);
        page.cpu = 4.0;
        page.tabs = vec![tab(7), tab(8)];
        let usage = Overview::new(vec![process(1, "Browser", "", 999), page]).tab_usage();
        assert_eq!(
            usage[&7],
            TabUsage {
                cpu: 2.0,
                memory: 200
            }
        );
        assert_eq!(usage.len(), 2);
    }

    #[test]
    fn parses_the_bridge_snapshot() {
        let json = r#"[{"id":1,"pid":10,"kind":"Browser","private":false,"title":null,"site":"",
            "owner":null,"cpu":2.5,"memory":2048,"spare":false,"tabs":[],"frames":[]},
            {"id":2,"pid":11,"kind":"WebContent","private":false,"title":"Google","site":"google.com",
            "owner":null,"cpu":0.0,"memory":4096,"spare":false,
            "tabs":[{"view":7,"title":"hi - Google Search"}],
            "frames":[{"name":"ads.example/frame","url":"https://ads.example/frame","sandboxed":true}]}]"#;
        let processes = parse(json).unwrap();
        assert_eq!(processes.len(), 2);
        assert_eq!(processes[1].tabs[0].view, 7);
        assert!(processes[1].frames[0].sandboxed);
        assert!(parse("{").is_err());
    }

    #[test]
    fn groups_browser_sites_and_spare_processes() {
        let mut spare = process(5, "WebContent", "", 10);
        spare.spare = true;
        let mut private = process(6, "WebContent", "example.com", 30);
        private.private = true;
        let mut media = process(7, "MediaServer", "", 5);
        media.owner = Some(3);
        let overview = Overview::new(vec![
            process(1, "Compositor", "", 50),
            process(2, "Browser", "", 20),
            process(3, "WebContent", "example.com", 100),
            process(4, "RequestServer", "example.com", 40),
            spare,
            private,
            media,
        ]);
        let ids = |processes: &[Process]| processes.iter().map(|p| p.id).collect::<Vec<_>>();
        assert_eq!(ids(&overview.browser), [2, 1]);
        assert_eq!(overview.sites.len(), 2);
        assert_eq!(overview.sites[0].name, "example.com");
        assert_eq!(ids(&overview.sites[0].processes), [3, 4, 7]);
        assert_eq!(overview.sites[0].memory, 145);
        assert_eq!(overview.sites[1].name, "example.com (Private)");
        assert_eq!(ids(&overview.spare), [5]);
        assert_eq!(overview.count, 7);
        assert_eq!(overview.memory, 255);
    }

    #[test]
    fn filters_by_name_site_tab_and_pid() {
        let mut page = process(3, "WebContent", "google.com", 100);
        page.tabs.push(ProcessTab {
            view: 1,
            title: "Search Results".into(),
        });
        let overview = Overview::new(vec![process(2, "Browser", "", 20), page]);
        assert_eq!(overview.filtered("GOOGLE").sites.len(), 1);
        assert!(overview.filtered("google").browser.is_empty());
        assert_eq!(overview.filtered("results").sites.len(), 1);
        assert_eq!(overview.filtered("102").browser.len(), 1);
        assert!(overview.filtered("nothing").is_empty());
        assert_eq!(overview.filtered("nothing").count, 2);
    }

    #[test]
    fn names_and_notes_match_ladybird() {
        let mut server = process(1, "RequestServer", "", 0);
        server.title = Some("Private".into());
        server.private = true;
        assert_eq!(server.name(false), "RequestServer — Private (Private)");
        assert_eq!(server.name(true), "RequestServer");
        let frame = |sandboxed| ProcessFrame {
            name: "a".into(),
            url: "b".into(),
            sandboxed,
        };
        server.frames = vec![frame(true), frame(false)];
        assert_eq!(server.note().as_deref(), Some("2 frames, 1 sandboxed"));
        assert!(!process(1, "Browser", "", 0).can_end());
    }

    #[test]
    fn formats_like_the_task_manager() {
        assert_eq!(format_memory(512), "512 bytes");
        assert_eq!(format_memory(1536), "1.5 KB");
        assert_eq!(format_memory(214 * 1024 * 1024), "214.0 MB");
        assert_eq!(format_cpu(2.46), "2.5%");
        assert_eq!(plural(3, "process"), "3 processes");
        assert_eq!(plural(1, "site"), "1 site");
    }
}
