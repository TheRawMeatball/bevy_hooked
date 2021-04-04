use std::{any::TypeId, cell::RefCell, marker::PhantomData, ops::Deref, sync::Arc};

use bevy::{ecs::component::Component, prelude::*, utils::HashMap};

use crate::internal::{EffectResolver, MountedId, Tx};

pub struct Fctx<'a> {
    tx: Tx,
    id: MountedId,
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
        res_checks: &'a mut HashMap<TypeId, (fn(&World) -> bool, Vec<MountedId>)>,
        cmp_checks: &'a mut HashMap<MountedId, Vec<fn(&mut World, MountedId) -> bool>>,
        world: &'a mut World,
    ) -> Self {
        Self {
            tx,
            id,
            res_checks: Some(RefCell::new(res_checks)),
            cmp_checks: Some(RefCell::new(cmp_checks)),
            init: true,
            world,
            nonsend_queue: RefCell::default(),
        }
    }

    pub(crate) fn update(tx: Tx, id: MountedId, world: &'a mut World) -> Self {
        Self {
            tx,
            id,
            init: false,
            res_checks: None,
            cmp_checks: None,
            world,
            nonsend_queue: RefCell::default(),
        }
    }

    // User facing hooks
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

    pub fn use_resource_setter<T: Component>(&self) -> Setter<T> {
        Setter {
            tx: self.tx.clone(),
            e: None,
            _m: PhantomData,
        }
    }

    pub fn use_linked_state<T: Component, F: FnOnce() -> T>(
        &self,
        f: F,
    ) -> (Ref<'_, T>, Setter<T>) {
        (
            if self.init {
                let rc = Arc::new(f());
                let entity = self.id.0;
                let rc_clone = rc.clone();
                self.nonsend_queue.borrow_mut().push(Box::new(move |world| {
                    world
                        .entity_mut(entity)
                        .insert(Arc::try_unwrap(rc_clone).ok().unwrap());
                }));
                self.cmp_checks
                    .as_ref()
                    .unwrap()
                    .borrow_mut()
                    .entry(self.id)
                    .or_default()
                    .push(|world, e| world.entity_mut(e.0).get_mut::<T>().unwrap().is_changed());
                Ref::Rc(rc)
            } else {
                let val = self.world.entity(self.id.0).get::<T>().unwrap();
                Ref::Borrowed(val)
            },
            Setter {
                tx: self.tx.clone(),
                e: Some(self.id),
                _m: PhantomData,
            },
        )
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

    pub fn use_self(&self) -> Entity {
        self.id.0
    }
}

pub enum Ref<'a, T> {
    Rc(Arc<T>),
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

pub struct Setter<T: Component> {
    tx: Tx,
    e: Option<MountedId>,
    _m: PhantomData<fn() -> T>,
}

impl<T: Component> Setter<T> {
    pub fn set<F: FnOnce(Mut<T>) + 'static>(&self, f: F) {
        if let Some(e) = self.e {
            self.tx
                .send(EffectResolver::MountedAccess(
                    e,
                    Box::new(move |w| f(w.entity_mut(e.0).get_mut().unwrap())),
                ))
                .unwrap();
        } else {
            self.tx
                .send(EffectResolver::ResourceAccess(
                    TypeId::of::<T>(),
                    Box::new(|w| f(w.get_resource_mut().unwrap())),
                ))
                .unwrap();
        }
    }
}

impl<'a> Drop for Fctx<'a> {
    fn drop(&mut self) {
        for nonsend in self.nonsend_queue.get_mut().drain(..) {
            nonsend(self.world);
        }
    }
}
