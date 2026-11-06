use super::Launcher;
use mirui::app::RendererFactory;
use mirui::app::plugin::Plugin;
use mirui::input::event::focus::FocusState;
use mirui::input::event::input::{KEY_BACKSPACE, KEY_RETURN, Key};
use mirui::prelude::*;
use mirui::surface::Surface;
use mirui::ui::text_input::{InputMethod, TextInputChanged};

#[component]
pub(crate) struct InputEditor;

#[derive(Clone)]
pub(crate) enum Edit {
    Insert(String),
    Preedit(String),
    Cancel,
    Backspace,
    Submit,
}

#[event]
fn session_changed(event: event!(TextInputChanged), launcher: model!(Launcher)) {
    launcher.text_session(event.state.map(|state| state.session));
}

pub(crate) fn install<B: Surface, F: RendererFactory<B>>(
    app: &mut App<B, F>,
    launcher: super::LauncherHandle,
) {
    app.with_input_method(InputMethod::software(session_changed::callback(
        launcher.clone(),
    )));
    app.add_system(edit::system(launcher.clone()));
    app.add_plugin(EditorFocus(launcher));
}

struct EditorFocus(super::LauncherHandle);
impl<B: Surface, F: RendererFactory<B>> Plugin<B, F> for EditorFocus {
    fn build(&mut self, _app: &mut App<B, F>) {}
    fn pre_render(&mut self, world: &mut mirui::ecs::World) {
        focus_editor(world, self.0.clone());
    }
}

#[system(order = Animation)]
fn edit(launcher: model!(Launcher), commands: commands!()) {
    if let Some(session) = launcher.input_session() {
        let management = launcher.management();
        let naming = management.editing();
        if naming && !management.text().prepared() {
            return;
        }
        if !naming && !launcher.wifi_keyboard() {
            return;
        }
        let edit = if naming {
            management.text().edit()
        } else {
            launcher.input_edit()
        };
        if let Some(edit) = edit {
            if naming {
                management.text().finish_edit();
            } else {
                launcher.finish_input_edit();
            }
            match edit {
                Edit::Insert(text) => commands.text_input().commit(session, text),
                Edit::Preedit(text) => commands.text_input().preedit(session, text, None),
                Edit::Cancel => commands.text_input().cancel(session),
                Edit::Backspace | Edit::Submit => commands.text_input().key(
                    session,
                    Key {
                        code: if matches!(edit, Edit::Submit) {
                            KEY_RETURN
                        } else {
                            KEY_BACKSPACE
                        },
                        pressed: true,
                        physical: None,
                        modifiers: None,
                        repeat: Some(false),
                    },
                ),
            }
        }
    }
}

fn focus_editor(world: &mut mirui::ecs::World, launcher: super::LauncherHandle) {
    let target = if launcher.wifi_keyboard() || launcher.management().editing() {
        world.query::<InputEditor>().iter().find_map(|(entity, _)| {
            let mut node = entity;
            loop {
                if mirui::ui::branch::is_effectively_hidden(world, node) {
                    return None;
                }
                let Some(parent) = world.get::<mirui::ui::Parent>(node) else {
                    break;
                };
                node = parent.0;
            }
            Some(entity)
        })
    } else {
        None
    };
    if launcher.management().editing() {
        let text = launcher.management().text();
        if !text.prepared()
            && let Some(entity) = target
        {
            let end = text.value().len();
            if let Some(editor) = world.get_mut::<mirui::ui::widgets::TextEditing>(entity)
                && editor
                    .set_selection(mirui::ui::widgets::text_editing::TextSelection {
                        anchor: end,
                        cursor: end,
                    })
                    .is_ok()
            {
                text.prepare_editor();
            }
        }
    }
    let current = world
        .resource::<FocusState>()
        .and_then(|focus| focus.focused);
    if (target.is_some() || current.is_some_and(|entity| world.has::<InputEditor>(entity)))
        && let Some(focus) = world.resource_mut::<FocusState>()
    {
        focus.focused = target;
    }
}
