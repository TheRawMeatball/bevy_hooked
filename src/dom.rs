use bevy::{
    ecs::world::EntityMut,
    prelude::{
        BuildWorldChildren, ButtonBundle, Children, Color, Entity, Handle, ImageBundle, NodeBundle,
        Parent, TextBundle, World,
    },
    text::{Font, Text, TextStyle},
    ui::{FlexDirection, Style},
};

use crate::FontHandle;

#[derive(Clone, Debug)]
pub enum Primitive {
    Node,
    Text(String),
    Image,
    Button,
}

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
pub struct PrimitiveId(Entity);

pub struct Dom<'a> {
    pub(crate) world: &'a mut World,
    pub(crate) cursor: u32,
}

impl<'a> Dom<'a> {
    pub fn set_cursor(&mut self, pos: u32) {
        self.cursor = pos;
    }
    pub fn get_cursor(&mut self) -> u32 {
        self.cursor
    }
    pub fn mount_as_child(
        &mut self,
        primitive: Primitive,
        parent: Option<PrimitiveId>,
    ) -> PrimitiveId {
        let font = self.world.get_resource::<FontHandle>().unwrap().0.clone();
        let mut entity = self.world.spawn();
        helper(&mut entity, primitive, font);
        let id = entity.id();
        if let Some(pid) = parent {
            self.world.entity_mut(pid.0).push_children(&[id]);
        }
        PrimitiveId(id)
    }
    pub fn diff_primitive(&mut self, old: PrimitiveId, new: Primitive) {
        let font = self.world.get_resource::<FontHandle>().unwrap().0.clone();
        let mut entity = self.world.entity_mut(old.0);
        let kind = entity.remove::<PrimitiveKind>().unwrap();
        match kind {
            PrimitiveKind::Node => {
                entity.remove_bundle::<NodeBundle>();
            }
            PrimitiveKind::Text => {
                entity.remove_bundle::<TextBundle>();
            }
            PrimitiveKind::Image => {
                entity.remove_bundle::<ImageBundle>();
            }
            PrimitiveKind::Button => {
                entity.remove_bundle::<ButtonBundle>();
            }
        }
        helper(&mut entity, new, font);
    }
    pub fn remove(&mut self, id: PrimitiveId) {
        if let Some(parent) = self.world.entity_mut(id.0).get::<Parent>().copied() {
            let mut children = self
                .world
                .entity_mut(parent.0)
                .get_mut::<Children>()
                .unwrap();
            let new = children
                .iter()
                .copied()
                .filter(|e| *e != id.0)
                .collect::<Vec<_>>();
            *children = Children::with(&new);
        }
        self.world.despawn(id.0);
    }
}

fn helper(entity: &mut EntityMut, primitive: Primitive, font: Handle<Font>) {
    let kind = match primitive {
        Primitive::Node => {
            entity.insert_bundle(NodeBundle {
                style: Style {
                    flex_direction: FlexDirection::ColumnReverse,
                    ..Default::default()
                },
                ..Default::default()
            });
            PrimitiveKind::Node
        }
        Primitive::Text(value) => {
            entity.insert_bundle(TextBundle {
                text: Text::with_section(
                    value,
                    TextStyle {
                        font,
                        font_size: 30.,
                        color: Color::BLACK,
                    },
                    Default::default(),
                ),
                ..Default::default()
            });
            PrimitiveKind::Text
        }
        Primitive::Image => {
            entity.insert_bundle(ImageBundle {
                ..Default::default()
            });
            PrimitiveKind::Image
        }
        Primitive::Button => {
            entity.insert_bundle(ButtonBundle {
                ..Default::default()
            });
            PrimitiveKind::Button
        }
    };
    entity.insert(kind);
}

enum PrimitiveKind {
    Node,
    Text,
    Image,
    Button,
}
