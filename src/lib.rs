mod dom;
mod fctx;
mod internal;

use bevy::{
    ecs::world,
    prelude::{AppBuilder, AssetServer, Handle, HandleUntyped, IntoExclusiveSystem, Plugin, World},
    text::Font,
};

use internal::Element;

use prelude::{Context, Dom};

pub mod prelude {
    use super::*;
    pub use fctx::Fctx;
    pub use internal::{ComponentFunc, Context, Element};
    pub mod e {
        pub use super::internal::{node, text};
    }
    pub use crate::HookedUiPlugin;
    pub use dom::{Dom, Primitive, PrimitiveId};
}

pub struct HookedUiPlugin(pub fn() -> Element);

pub(crate) struct FontHandle(Handle<Font>);

impl Plugin for HookedUiPlugin {
    fn build(&self, app: &mut AppBuilder) {
        let mut ctx = Context::new();
        let world = app.world_mut();

        let font_asset = world
            .get_resource::<AssetServer>()
            .unwrap()
            .load("FiraMono-Medium.ttf");

        world.insert_resource(FontHandle(font_asset));

        ctx.mount_root((self.0)(), &mut Dom { world, cursor: 0 });
        app.insert_non_send_resource(ctx);
        app.add_system(
            (|world: &mut World| {
                let mut ctx = world.remove_non_send::<Context>().unwrap();

                ctx.process_messages(&mut Dom { world, cursor: 0 });

                world.insert_non_send(ctx);
            })
            .exclusive_system(),
        );
    }
}
