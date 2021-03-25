use std::{
    any::{Any, TypeId},
    cell::{Cell, RefCell},
    marker::PhantomData,
    ops::Deref,
    rc::Rc,
};

use bevy::{ecs::component::Component, prelude::*, utils::HashMap};

use crate::internal::{Effect, EffectResolver, EffectStage, MountedId, Tx};

pub struct Fctx<'a> {
    tx: Tx,
    id: MountedId,
    states: RefCell<&'a mut Vec<Rc<dyn Any + Send + Sync>>>,
    memos: RefCell<&'a mut Vec<Option<Rc<Memo>>>>,
    effects: RefCell<&'a mut Vec<Effect>>,
    states_selector: Cell<usize>,
    effects_selector: Cell<usize>,
    memos_selector: Cell<usize>,
    res_checks: Option<RefCell<&'a mut HashMap<TypeId, (fn(&World) -> bool, Vec<MountedId>)>>>,
    cmp_checks: Option<RefCell<&'a mut HashMap<MountedId, Vec<fn(&mut World, MountedId) -> bool>>>>,
    init: bool,
    world: &'a mut World,
    nonsend_queue: RefCell<Vec<Box<dyn FnOnce(&mut World)>>>,
}

impl<'a> Fctx<'a> {
    // Internal stuff
    pub(crate) fn render_first(
        tx: Tx,
        id: MountedId,
        states: &'a mut Vec<Rc<dyn Any + Send + Sync>>,
        memos: &'a mut Vec<Option<Rc<Memo>>>,
        effects: &'a mut Vec<Effect>,
        res_checks: &'a mut HashMap<TypeId, (fn(&World) -> bool, Vec<MountedId>)>,
        cmp_checks: &'a mut HashMap<MountedId, Vec<fn(&mut World, MountedId) -> bool>>,
        world: &'a mut World,
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
            cmp_checks: Some(RefCell::new(cmp_checks)),
            init: true,
            world,
            nonsend_queue: RefCell::default(),
        }
    }

    pub(crate) fn update(
        tx: Tx,
        id: MountedId,
        states: &'a mut Vec<Rc<dyn Any + Send + Sync>>,
        memos: &'a mut Vec<Option<Rc<Memo>>>,
        effects: &'a mut Vec<Effect>,
        world: &'a mut World,
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
            cmp_checks: None,
            world,
            nonsend_queue: RefCell::default(),
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
            Ref::Rc(rc)
        } else {
            let states = self.states.borrow();
            let state = states.get(self.states_selector.get()).unwrap();
            Ref::Rc(Rc::downcast(state.clone()).unwrap())
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
            let rc = Rc::new(Memo {
                eq_cache: Box::new(eq_cache),
                val: Rc::new(f()),
            });
            memos.push(Some(rc.clone()));
            rc
        } else {
            let mut state = memos
                .get_mut(self.memos_selector.get())
                .unwrap()
                .take()
                .unwrap();
            if state.eq_cache.downcast_ref::<X>().unwrap() != &eq_cache {
                let mut memo: &mut Memo = Rc::get_mut(&mut state).unwrap();
                memo.val = Rc::new(f());
                memo.eq_cache = Box::new(eq_cache);
            }
            *memos.get_mut(self.memos_selector.get()).unwrap() = Some(state.clone());
            state.clone()
        };
        self.memos_selector.set(self.memos_selector.get() + 1);
        Ref::Rc(Rc::downcast(memo.val.clone()).unwrap())
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

    pub fn use_linked_state<T: Component, F: FnOnce() -> T>(&self, f: F) -> Ref<'_, T> {
        if self.init {
            let rc = Rc::new(f());
            let entity = self.id.0;
            let rc_clone = rc.clone();
            self.nonsend_queue.borrow_mut().push(Box::new(move |world| {
                world
                    .entity_mut(entity)
                    .insert(Rc::try_unwrap(rc_clone).ok().unwrap());
            }));
            self.cmp_checks
                .as_ref()
                .unwrap()
                .borrow_mut()
                .entry(self.id)
                .or_default()
                .push(|world, e| world.entity_mut(e.0).get_mut::<T>().unwrap().is_changed());
            Ref::new(rc)
        } else {
            let val = self.world.entity(self.id.0).get::<T>().unwrap();
            Ref::Borrowed(val)
        }
    }

    pub fn use_broadcast_state<T: Component>(&self, v: T) {
        let entity = self.id.0;
        self.nonsend_queue.borrow_mut().push(Box::new(move |world| {
            world.entity_mut(entity).insert(v);
        }));
    }

    pub fn use_disconnected_state<T: Component, F: FnOnce() -> T>(&self, f: F) {
        if self.init {
            let v = f();
            let entity = self.id.0;
            self.nonsend_queue.borrow_mut().push(Box::new(move |world| {
                world.entity_mut(entity).insert(v);
            }));
        }
    }
}

pub enum Ref<'a, T> {
    Rc(Rc<T>),
    Borrowed(&'a T),
}

impl<'a, T: 'static> Deref for Ref<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        match self {
            Ref::Rc(v) => v,
            Ref::Borrowed(v) => *v,
        }
    }
}

impl<'a, T> Ref<'a, T> {
    fn new(inner: Rc<T>) -> Self {
        Self::Rc(inner)
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

impl<'a> Drop for Fctx<'a> {
    fn drop(&mut self) {
        for nonsend in self.nonsend_queue.get_mut().drain(..) {
            nonsend(self.world);
        }
    }
}
