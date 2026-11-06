use mirui::ecs::{Entity, World};
use mirui::input::event::scroll::ScrollOffset;
use mirui::types::Fixed;
use mirui::ui::property::{Property, PropertyChange};

pub(super) struct Position;

impl Property for Position {
    type Value = Fixed;

    fn apply(world: &mut World, entity: Entity, y: Fixed) -> PropertyChange {
        if world
            .get::<ScrollOffset>(entity)
            .is_some_and(|offset| offset.x == Fixed::ZERO && offset.y == y)
        {
            return PropertyChange::Unchanged;
        }
        world.insert(entity, ScrollOffset { x: Fixed::ZERO, y });
        PropertyChange::Visual
    }
}
