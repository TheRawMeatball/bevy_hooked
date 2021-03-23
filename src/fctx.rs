use std::{
    any::{Any, TypeId},
    cell::{Cell, RefCell},
    marker::PhantomData,
    ops::Deref,
    rc::Rc,
};

use bevy::{ecs::component::Component, prelude::World, utils::HashMap};

use crate::internal::{Effect, EffectResolver, EffectStage, MountedId, Tx};

pub struct Fctx<'a> {
    tx: Tx,
    id: MountedId,
    states: RefCell<&'a mut Vec<Rc<dyn Any + Send + Sync>>>,
    memos: RefCell<&'a mut Vec<Rc<RefCell<Memo>>>>,
    effects: RefCell<&'a mut Vec<Effect>>,
    states_selector: Cell<usize>,
    effects_selector: Cell<usize>,
    memos_selector: Cell<usize>,
    res_checks: Option<RefCell<&'a mut HashMap<TypeId, (fn(&World) -> bool, Vec<MountedId>)>>>,
    init: bool,
    world: &'a World,
}

impl<'a> Fctx<'a> {
    // Internal stuff
    pub(crate) fn render_first(
        tx: Tx,
        id: MountedId,
        states: &'a mut Vec<Rc<dyn Any + Send + Sync>>,
        memos: &'a mut Vec<Rc<RefCell<Memo>>>,
        effects: &'a mut Vec<Effect>,
        res_checks: &'a mut HashMap<TypeId, (fn(&World) -> bool, Vec<MountedId>)>,
        world: &'a World,
    ) -> Self {
        Self {
            tx,
            id,
            states: RefCell::new(states),
            memos: RefCell::new(memos),
            effects: RefCell::new(effects),
            states_selector: Cell::new(0),
            effects_selector: Cell::new(0),
            memos_selector: Cell::new(0),
            res_checks: Some(RefCell::new(res_checks)),
            init: true,
            world,
        }
    }

    pub(crate) fn update(
        tx: Tx,
        id: MountedId,
        states: &'a mut Vec<Rc<dyn Any + Send + Sync>>,
        memos: &'a mut Vec<Rc<RefCell<Memo>>>,
        effects: &'a mut Vec<Effect>,
        world: &'a World,
    ) -> Self {
        Self {
            tx,
            id,
            states: RefCell::new(states),
            memos: RefCell::new(memos),
            effects: RefCell::new(effects),
            states_selector: Cell::new(0),
            effects_selector: Cell::new(0),
            memos_selector: Cell::new(0),
            init: false,
            res_checks: None,
            world,
        }
    }

    // User facing hooks

    pub fn use_state<'f, T: Send + Sync + 'static, F: Fn() -> T>(
        &'f self,
        default: F,
    ) -> (Ref<'f, T>, Setter<T>) {
        let state = if self.init {
            let rc = Rc::new(default());
            self.states.borrow_mut().push(rc.clone());
            Ref::new(rc)
        } else {
            let states = self.states.borrow();
            let state = states.get(self.states_selector.get()).unwrap();
            Ref::new(state.clone())
        };
        self.states_selector.set(self.states_selector.get() + 1);
        (
            state,
            Setter {
                tx: self.tx.clone(),
                target: self.id,
                state: self.states_selector.get() - 1,
                _m: PhantomData,
            },
        )
    }

    pub fn use_effect<F, D, X>(&self, eq_cache: Option<X>, f: F)
    where
        F: FnOnce() -> D + Send + Sync + 'static,
        D: FnOnce() + Send + Sync + 'static,
        X: PartialEq + 'static,
    {
        let mut effects = self.effects.borrow_mut();
        if self.init {
            effects.push(Effect {
                eq_cache: eq_cache.map(|x| Box::new(x) as Box<dyn Any>),
                f: EffectStage::Effect(Box::new(move || Box::new(f()))),
            });
        } else {
            if effects
                .get(self.effects_selector.get())
                .and_then(|v| v.eq_cache.as_ref())
                .and_then(|v| (&**v).downcast_ref::<X>())
                .zip(eq_cache.as_ref())
                .map(|(v, f)| v != f)
                .unwrap_or(true)
            {
                let old = effects.get_mut(self.effects_selector.get()).unwrap();
                replace_with::replace_with_or_abort(old, move |v| {
                    match v.f {
                        EffectStage::Effect(_) => {}
                        EffectStage::Destructor(d) => {
                            d();
                        }
                    }
                    Effect {
                        eq_cache: eq_cache.map(|x| Box::new(x) as Box<dyn Any>),
                        f: EffectStage::Effect(Box::new(move || Box::new(f()))),
                    }
                });
            }
            self.effects_selector.set(self.effects_selector.get() + 1);
        }
    }

    pub fn use_memo<'f, X, T, F>(&'f self, eq_cache: X, f: F) -> Ref<'f, T>
    where
        T: 'static,
        X: PartialEq + 'static,
        F: Fn() -> T,
    {
        let mut memos = self.memos.borrow_mut();
        let memo = if self.init {
            let rc = Rc::new(RefCell::new(Memo {
                eq_cache: Box::new(eq_cache),
                val: Rc::new(f()),
            }));
            memos.push(rc.clone());
            rc
        } else {
            let state = memos.get(self.memos_selector.get()).unwrap();
            if memos
                .get(self.memos_selector.get())
                .and_then(|v| {
                    v.borrow()
                        .eq_cache
                        .as_ref()
                        .downcast_ref::<X>()
                        .map(|v| v != &eq_cache)
                })
                .unwrap()
            {
                let mut memo = state.borrow_mut();
                memo.val = Rc::new(f());
                memo.eq_cache = Box::new(eq_cache);
            }
            state.borrow().val.clone()
        };
        self.memos_selector.set(self.memos_selector.get() + 1);
        Ref::new(memo)
    }

    pub fn use_resource<T: Component>(&self) -> &T {
        if let Some(c) = &self.res_checks {
            c.borrow_mut()
                .entry(std::any::TypeId::of::<T>())
                .or_insert_with(|| (World::is_resource_changed::<T>, Vec::new()))
                .1
                .push(self.id);
        }
        self.world.get_resource().unwrap()
    }
}

pub struct Ref<'a, T> {
    inner: Rc<dyn Any>,
    _lt: PhantomData<&'a T>,
}

impl<'a, T: 'static> Deref for Ref<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        (&*self.inner).downcast_ref().unwrap()
    }
}

impl<'a, T> Ref<'a, T> {
    fn new(inner: Rc<dyn Any>) -> Self {
        Self {
            inner,
            _lt: PhantomData,
        }
    }
}

pub(crate) struct Memo {
    eq_cache: Box<dyn Any>,
    val: Rc<dyn Any>,
}

pub struct Setter<T> {
    tx: Tx,
    target: MountedId,
    state: usize,
    _m: PhantomData<fn() -> T>,
}

impl<T: 'static> Setter<T> {
    pub fn set<F: FnOnce(&mut T) + Send + Sync + 'static>(&self, f: F) {
        let id = self.target;
        let state = self.state;
        self.tx
            .send(EffectResolver {
                f: Box::new(|c| f(c.downcast_mut().unwrap())),
                target_component: id,
                target_state: Some(state as u64),
            })
            .unwrap();
    }
}
