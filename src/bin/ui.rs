use bevy::prelude::*;
use bevy_hooked::prelude::*;

fn main() {
    App::build()
        .add_plugins(DefaultPlugins)
        .add_plugin(HookedUiPlugin(app))
        .add_startup_system(
            (|mut commands: Commands| {
                commands.spawn(UiCameraBundle::default());
            })
            .system(),
        )
        .run();
}

pub fn simple_blinker(ctx: Fctx, period: &u64) -> Element {
    let (is_on, set_is_on) = ctx.use_state(|| false);
    let period = *period;
    ctx.use_effect(Some(period), move || {
        let (tx, rx) = crossbeam_channel::bounded(1);
        std::thread::spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_secs(period));
            set_is_on.set(|state| {
                *state = !*state;
            });
            if rx.try_recv().is_ok() {
                break;
            }
        });
        move || tx.send(()).unwrap()
    });

    if *is_on {
        e::text(format!("Yay! - Period = {}", period))
    } else {
        e::text(format!("Nay! - Period = {}", period))
    }
}

pub fn full_blinker(ctx: Fctx) -> Option<Element> {
    let (is_on, set_is_on) = ctx.use_state(|| true);
    ctx.use_effect(Some(()), move || {
        let (tx, rx) = crossbeam_channel::bounded(1);
        std::thread::spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_secs(1));
            set_is_on.set(|state| {
                *state = !*state;
            });
            if rx.try_recv().is_ok() {
                break;
            }
        });
        move || tx.send(()).unwrap()
    });

    if *is_on {
        Some(e::text("hi"))
    } else {
        None
    }
}

pub fn counter(ctx: Fctx) -> Element {
    let (state, state_setter) = ctx.use_state(|| 0);
    ctx.use_effect(Some(()), || {
        let (tx, rx) = crossbeam_channel::bounded(1);
        std::thread::spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_secs(1));
            state_setter.set(|state| {
                *state += 1;
            });
            if rx.try_recv().is_ok() {
                break;
            }
        });
        move || tx.send(()).unwrap()
    });

    e::text(format!("{} seconds since creation!", &*state))
}

fn app() -> Element {
    e::node([
        counter.e(()),
        //full_blinker.e(()),
        simple_blinker.e((3,)),
        simple_blinker.e((5,)),
    ])
}
