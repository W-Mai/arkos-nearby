use crate::model::{GameRow, RoomRow};
use mirui::core::model::ModelList;
use mirui::prelude::*;

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Key {
    Named(String),
    Slot(usize),
    Anonymous { revision: u64, index: usize },
}

#[derive(Clone, Copy)]
pub(super) struct TextChanged(bool);

impl TextChanged {
    fn contains(self, other: Self) -> bool {
        self.0 && other.0
    }
}

#[model(change = TextChanged, watch(text = TextChanged(true)))]
pub(super) struct Row {
    key: Key,
    name: String,
    detail: String,
    room: bool,
    #[observe]
    title_revision: u64,
    #[observe]
    detail_revision: u64,
    artwork: String,
    pub cover_bounds: model!(owned super::artwork::CoverBounds),
    pub static_picture: model!(owned super::static_snapshot::Picture),
    #[observe]
    pub index: usize,
    #[observe]
    pub strength: u8,
    #[observe]
    pub available: bool,
    #[observe]
    pub directory: bool,
    #[observe]
    pub icon: u8,
    #[observe]
    pub platform: super::consoles::Platform,
    #[observe]
    pub favorite: bool,
}

#[model]
impl Row {
    pub fn artwork(&self) -> String {
        self.artwork.clone()
    }

    fn text_value(&self, title: bool) -> String {
        if title {
            self.name.clone()
        } else if self.available || !self.room {
            self.detail.clone()
        } else {
            "房间已离线，等待重新出现".into()
        }
    }
}

impl RowHandle {
    pub fn artwork_token(&self) -> Option<u64> {
        self.cover_bounds().token()
    }

    pub fn label(&self, title: bool) -> String {
        if title {
            self.title_revision();
        } else {
            self.detail_revision();
            self.available();
        }
        self.text_value(title)
    }
}

impl Row {
    pub fn room(index: usize, data: &RoomRow) -> Self {
        Self {
            key: Key::Named(data.key.clone()),
            index,
            name: data.name.clone(),
            detail: data.detail.clone(),
            room: true,
            title_revision: 0,
            detail_revision: 0,
            artwork: String::new(),
            cover_bounds: super::artwork::CoverBounds::default().into(),
            static_picture: super::static_snapshot::Picture::default().into(),
            strength: data.signal,
            available: data.available,
            directory: false,
            icon: 0,
            platform: super::consoles::Platform::Unknown,
            favorite: false,
        }
    }
    pub fn game(index: usize, data: &GameRow) -> Self {
        Self {
            key: Key::Named(data.key.clone()),
            index,
            name: data.name.clone(),
            detail: data.detail.clone(),
            room: false,
            title_revision: 0,
            detail_revision: 0,
            artwork: data.artwork.clone(),
            cover_bounds: super::artwork::CoverBounds::default().into(),
            static_picture: super::static_snapshot::Picture::default().into(),
            strength: 0,
            available: data.available,
            directory: data.directory,
            platform: super::consoles::Platform::from_id(&data.icon),
            favorite: data.favorite,
            icon: match data.icon.as_str() {
                "gb" | "gbc" | "gba" => 1,
                "arcade" | "neogeo" | "cps1" | "cps2" | "cps3" => 2,
                _ => 0,
            },
        }
    }
}

// Normalize ambiguous transport keys at ingestion; ModelList owns member identities.
pub(super) fn reconcile_slots(list: &mut ModelList<Row>, mut rows: Vec<Row>, revision: u64) {
    // Catalog slots own display bindings; game identity stays in the captured View.
    for row in &mut rows {
        row.key = Key::Slot(row.index);
    }
    reconcile(list, rows, revision);
}

pub(super) fn reconcile(list: &mut ModelList<Row>, mut rows: Vec<Row>, revision: u64) {
    let ambiguous: Vec<_> = rows
        .iter()
        .map(|row| {
            matches!(&row.key, Key::Named(key) if key.is_empty())
                || rows.iter().filter(|other| other.key == row.key).count() > 1
        })
        .collect();
    for (row, ambiguous) in rows.iter_mut().zip(ambiguous) {
        if ambiguous {
            row.key = Key::Anonymous {
                revision,
                index: row.index,
            };
        }
    }
    list.reconcile(
        rows,
        |row| row.key.clone(),
        |current, mut incoming| {
            // Collection merges can notify named watches even when their text
            // is unchanged. Observe content revisions independently of order.
            incoming.title_revision = current
                .title_revision
                .wrapping_add(u64::from(current.name != incoming.name));
            incoming.detail_revision = current.detail_revision.wrapping_add(u64::from(
                current.detail != incoming.detail || current.room != incoming.room,
            ));
            std::mem::swap(&mut current.cover_bounds, &mut incoming.cover_bounds);
            std::mem::swap(&mut current.static_picture, &mut incoming.static_picture);
            *current = incoming;
        },
    )
    .unwrap_or_else(|failure| panic!("Invalid visible row collection: {:?}", failure.error()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use mirui::core::reactive::Computed;
    use std::{cell::Cell, rc::Rc};

    #[compose]
    fn observed_title(row: model!(Row), count: Rc<Cell<u32>>) -> Computed<String> {
        Computed::new(move || {
            count.set(count.get() + 1);
            row.label(true)
        })
    }

    #[test]
    fn retained_labels_observe_content_independently_of_window_position() {
        let mut app = App::headless(640, 480);
        app.with_default_widgets().with_default_systems();
        crate::render::install(&mut app).unwrap();
        let state = app
            .world
            .resource::<super::super::state::DisplayRuntime>()
            .unwrap()
            .state
            .clone();
        let mut view = crate::model::View {
            page: crate::model::Page::Games,
            entries: ["first", "second"]
                .map(|key| GameRow {
                    key: key.into(),
                    name: key.into(),
                    available: true,
                    ..Default::default()
                })
                .to_vec(),
            ..Default::default()
        };
        state.present(&view);
        let row = state.entries().entries().next().unwrap().1;
        let reads = Rc::new(Cell::new(0));
        let root = app.spawn_root().id();
        let title = app.compose(root, observed_title(row, reads.clone()));
        assert_eq!(title.get(), "first");
        view.entries.swap(0, 1);
        state.present(&view);
        assert_eq!(title.get(), "first");
        assert_eq!(reads.get(), 1);
        view.entries[1].favorite = true;
        view.entries[1].artwork = "changed.png".into();
        state.present(&view);
        assert_eq!(title.get(), "first");
        assert_eq!(reads.get(), 1);
        view.entries[1].name = "renamed".into();
        state.present(&view);
        assert_eq!(title.get(), "renamed");
        assert_eq!(reads.get(), 2);
    }
}
