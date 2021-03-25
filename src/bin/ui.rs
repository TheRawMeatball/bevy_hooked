use bevy::prelude::*;
use bevy_hooked::prelude::*;

fn main() {
    App::build()
        .add_plugins(DefaultPlugins)
        .add_plugin(HookedUiPlugin(app))
        .add_startup_system(
            (|mut commands: Commands| {
                commands.spawn_bundle(UiCameraBundle::default());
            })
            .system(),
        )
        // .add_system(debug_system.system())
        .add_system(blinker_system.system())
        .add_system(counter_system.system())
        .run();
}

pub fn simple_blinker(ctx: Fctx, period: &f32) -> Element {
    let is_on = ctx.use_linked_state(|| Blinker(false));
    ctx.use_disconnected_state(|| TimeSpent(0.));
    ctx.use_broadcast_state(Period(*period));

    if is_on.0 {
        e::text(format!("Yay! - Period = {}", period))
    } else {
        e::text(format!("Nay! - Period = {}", period))
    }
}

pub fn full_blinker(ctx: Fctx, period: &f32) -> Option<Element> {
    let is_on = ctx.use_linked_state(|| Blinker(false));
    ctx.use_disconnected_state(|| TimeSpent(0.));
    ctx.use_broadcast_state(Period(*period));

    if is_on.0 {
        Some(e::text("hi"))
    } else {
        None
    }
}

fn blinker_system(mut q: Query<(&mut TimeSpent, &mut Blinker, &Period)>, dt: Res<Time>) {
    for (mut time_spent, mut blink, period) in q.iter_mut() {
        time_spent.0 += dt.delta_seconds();
        if time_spent.0 > period.0 {
            time_spent.0 -= period.0;
            blink.0 = !blink.0;
        }
    }
}

pub fn counter(ctx: Fctx) -> Element {
    let state = ctx.use_linked_state(|| IntegerTimeSpent(0));
    ctx.use_disconnected_state(|| TimeSpent(0.));

    e::text(format!("{} seconds since creation!", state.0))
}

fn counter_system(mut q: Query<(&mut TimeSpent, &mut IntegerTimeSpent)>, dt: Res<Time>) {
    for (mut time_spent, mut integer_time_spent) in q.iter_mut() {
        time_spent.0 += dt.delta_seconds();
        let int = time_spent.0 as u32;
        if int != integer_time_spent.0 {
            integer_time_spent.0 = int;
        }
    }
}

struct Blinker(bool);
struct Period(f32);
struct TimeSpent(f32);
struct IntegerTimeSpent(u32);

fn app() -> Element {
    e::node([
        counter.e(()),
        full_blinker.e((1.,)),
        simple_blinker.e((3.,)),
        simple_blinker.e((5.,)),
    ])
}

#[allow(dead_code)]
fn debug_system(
    query: Query<(&PrimitiveKind, Option<&Text>, Option<&Children>)>,
    roots: Query<Entity, (With<PrimitiveKind>, Without<Parent>)>,
) {
    use std::io::Write;
    fn recursor(
        f: &mut impl std::io::Write,
        element: Entity,
        nest_level: i32,
        query: &Query<(&PrimitiveKind, Option<&Text>, Option<&Children>)>,
    ) -> std::io::Result<()> {
        for _ in 0..=nest_level {
            write!(f, "|>")?;
        }
        let (kind, text, children) = query.get(element).unwrap();
        match kind {
            PrimitiveKind::Text => writeln!(
                f,
                "[Text] {}",
                text.unwrap()
                    .sections
                    .iter()
                    .flat_map(|v| v.value.chars())
                    .collect::<String>()
            )?,
            PrimitiveKind::Node => writeln!(f, "[Node]")?,
            PrimitiveKind::Image => writeln!(f, "[Image]",)?,
            PrimitiveKind::Button => writeln!(f, "[Button]",)?,
        }
        for &child in children.iter().flat_map(|&v| v.iter()) {
            recursor(f, child, nest_level + 1, &query)?;
        }
        Ok(())
    }

    let stdout = std::io::stdout();
    let mut output = stdout.lock();
    writeln!(&mut output, "new root!").unwrap();
    for element in roots.iter() {
        recursor(&mut output, element, 0, &query).unwrap();
    }
}
