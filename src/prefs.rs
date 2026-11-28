//! What the settings page holds and where (plan 06; Tim, 2026-11-28, after T3 Code's
//! settings): sections in a rail, each a page of groups, each group a card of rows, and
//! each row its title and a line saying what it does. The rows are data here, so the
//! page, its keys and its clicks agree on what's where; the row picked is an index into
//! `rows` of the open section.

/// A section of the settings, picked in its rail or with ⌘1 to ⌘4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Section {
    #[default]
    Library,
    Playback,
    Look,
    KeysAndData,
}

impl Section {
    pub const ALL: [Section; 4] = [Section::Library, Section::Playback, Section::Look, Section::KeysAndData];

    pub fn label(self) -> &'static str {
        match self {
            Section::Library => "Library",
            Section::Playback => "Playback",
            Section::Look => "Look",
            Section::KeysAndData => "Keys & data",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Section::Library => "folder",
            Section::Playback => "headphones",
            Section::Look => "palette",
            Section::KeysAndData => "keyboard",
        }
    }

    /// The section ⌘ with `digit` opens, 1 to 4.
    pub fn numbered(digit: &str) -> Option<Section> {
        let n: usize = digit.parse().ok()?;
        Section::ALL.get(n.checked_sub(1)?).copied()
    }

    /// The next section, or the one before, round the rail.
    pub fn step(self, forward: bool) -> Section {
        let at = Section::ALL.iter().position(|&s| s == self).unwrap_or(0);
        let n = Section::ALL.len();
        Section::ALL[if forward { (at + 1) % n } else { (at + n - 1) % n }]
    }
}

/// A row of the settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row {
    /// A music folder, by its place in the settings' list.
    Folder(usize),
    AddFolder,
    /// A backing-track folder, by its place in its list.
    Backing(usize),
    AddBacking,
    Sort,
    Output,
    Appearance,
    Theme,
    Keyboard,
    DataFolder,
    /// Shown only while the data is kept somewhere else.
    DefaultDataFolder,
}

impl Row {
    /// Its name and what it does, for a row that's a setting; folder rows show the folder.
    pub fn title(self) -> &'static str {
        match self {
            Row::Folder(_) | Row::Backing(_) => "",
            Row::AddFolder => "Add folder…",
            Row::AddBacking => "Add backing-track folder…",
            Row::Sort => "Sort by",
            Row::Output => "Output",
            Row::Appearance => "Appearance",
            Row::Theme => "Theme",
            Row::Keyboard => "Keyboard",
            Row::DataFolder => "Data folder",
            Row::DefaultDataFolder => "Back to the default folder",
        }
    }

    pub fn about(self) -> &'static str {
        match self {
            Row::Sort => "Drag the fields into order. Grouped, the list groups by the first one.",
            Row::Output => "Where Odeion plays: the system's output, or a device of your choosing while it's connected.",
            Row::Appearance => "Light, dark, or Auto to follow the system.",
            Row::Theme => "Odeion's own colours, or an installed Zed theme (experimental).",
            Row::Keyboard => "The Mac's keys, or Vim's letters for moving and acting.",
            Row::DataFolder => "Where Odeion keeps playlists, sections, loops and its library. Another one reopens Odeion from it.",
            Row::DefaultDataFolder => "Reopens Odeion from its own folder.",
            Row::Folder(_) | Row::AddFolder | Row::Backing(_) | Row::AddBacking => "",
        }
    }

    /// Whether it lists a folder (with its count and a ×) rather than a setting.
    pub fn is_folder(self) -> bool {
        matches!(self, Row::Folder(_) | Row::Backing(_))
    }
}

/// A card of rows under a heading, perhaps with a line about them all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub heading: &'static str,
    pub about: &'static str,
    pub rows: Vec<Row>,
}

/// What decides which rows there are: how many folders of each kind, and whether the
/// data is kept somewhere other than the default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    pub folders: usize,
    pub backing: usize,
    pub custom_data: bool,
}

/// `section`'s page.
pub fn groups(section: Section, layout: Layout) -> Vec<Group> {
    let group = |heading, about, rows| Group { heading, about, rows };
    match section {
        Section::Library => vec![
            group(
                "Music folders",
                "Odeion lists every song in these, by its tags, or by its Artist/Album folders without them.",
                (0..layout.folders).map(Row::Folder).chain([Row::AddFolder]).collect(),
            ),
            group(
                "Backing tracks",
                "Jam tracks that aren't songs: kept out of the library, with a place of their own in the sidebar.",
                (0..layout.backing).map(Row::Backing).chain([Row::AddBacking]).collect(),
            ),
            group("Order", "", vec![Row::Sort]),
        ],
        Section::Playback => vec![group("Sound", "", vec![Row::Output])],
        Section::Look => vec![group("Colours", "", vec![Row::Appearance, Row::Theme])],
        Section::KeysAndData => vec![
            group("Keys", "", vec![Row::Keyboard]),
            group(
                "Data",
                "",
                std::iter::once(Row::DataFolder).chain(layout.custom_data.then_some(Row::DefaultDataFolder)).collect(),
            ),
        ],
    }
}

/// `section`'s rows in order, as ↑ ↓ go through them.
pub fn rows(section: Section, layout: Layout) -> Vec<Row> {
    groups(section, layout).into_iter().flat_map(|g| g.rows).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const TWO_AND_ONE: Layout = Layout { folders: 2, backing: 1, custom_data: false };

    #[test]
    fn the_library_lists_each_folder_then_the_order() {
        assert_eq!(
            rows(Section::Library, TWO_AND_ONE),
            [Row::Folder(0), Row::Folder(1), Row::AddFolder, Row::Backing(0), Row::AddBacking, Row::Sort]
        );
        // No folders yet: still a way to add one.
        let none = Layout { folders: 0, backing: 0, custom_data: false };
        assert_eq!(rows(Section::Library, none), [Row::AddFolder, Row::AddBacking, Row::Sort]);
    }

    #[test]
    fn going_back_to_the_default_data_folder_shows_only_away_from_it() {
        assert_eq!(rows(Section::KeysAndData, TWO_AND_ONE), [Row::Keyboard, Row::DataFolder]);
        let away = Layout { custom_data: true, ..TWO_AND_ONE };
        assert_eq!(rows(Section::KeysAndData, away), [Row::Keyboard, Row::DataFolder, Row::DefaultDataFolder]);
    }

    #[test]
    fn every_setting_says_what_it_does() {
        for section in Section::ALL {
            assert!(!groups(section, TWO_AND_ONE).is_empty());
            for row in rows(section, TWO_AND_ONE) {
                assert!(row.is_folder() || !row.title().is_empty(), "{row:?}");
                let action = matches!(row, Row::AddFolder | Row::AddBacking);
                assert!(row.is_folder() || action || !row.about().is_empty(), "{row:?}");
            }
        }
    }

    #[test]
    fn sections_go_by_number_and_round_the_rail() {
        assert_eq!(Section::numbered("1"), Some(Section::Library));
        assert_eq!(Section::numbered("4"), Some(Section::KeysAndData));
        assert_eq!(Section::numbered("5"), None);
        assert_eq!(Section::numbered("0"), None);
        assert_eq!(Section::KeysAndData.step(true), Section::Library);
        assert_eq!(Section::Library.step(false), Section::KeysAndData);
    }
}
