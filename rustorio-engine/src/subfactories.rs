//! This module contains utilities to manipulate past ticks. This is an advanced feature, that can
//! be used to build reuseable subfactories.
use std::{
    marker::PhantomData,
    ops::{AddAssign, Deref},
};

use crate::{
    ResourceType,
    resources::{Bundle, InsufficientResourceError, Resource},
    tick::{BackwardTickingError, Tick, TickSnapshot},
};

impl TickSnapshot {
    /// Advances the snapshot to the given tick, triggering a callback on each intermediate past tick.
    pub fn on_each_tick(
        &mut self,
        until: TickSnapshot,
        mut on_each_tick: impl for<'tick> FnMut(&PastTick<'tick>),
    ) -> Result<u64, BackwardTickingError> {
        if let Some(diff) = until.tick.checked_sub(self.tick) {
            while self.tick < until.tick {
                self.tick += 1;
                let past_tick = PastTick {
                    tick: *self,
                    phantom: PhantomData,
                };
                on_each_tick(&past_tick)
            }
            Ok(diff)
        } else {
            Err(BackwardTickingError)
        }
    }
}

/// A tick from the past. If a machine hasn't yet been updated beyond this tick, its resources can
/// be accessed in the past so that we can move resources around before catching up to the present.
/// This is used with `TickSnapshot`.
///
/// The `'tick` lifetime is invariant and prevents mixing resources the come from different times.
#[derive(Debug)]
pub struct PastTick<'tick> {
    tick: TickSnapshot,
    /// Makes the lifetime invariant, which makes it usable as a branding lifetime à la
    /// `GhostCell`.
    phantom: PhantomData<fn(&'tick ()) -> &'tick ()>,
}

impl<'tick> PastTick<'tick> {
    /// The past time that this corresponds to.
    pub const fn as_snapshot(&self) -> TickSnapshot {
        self.tick
    }
}

impl<'tick> From<&PastTick<'tick>> for TickSnapshot {
    fn from(tick: &PastTick<'tick>) -> Self {
        tick.as_snapshot()
    }
}

/// An `X` from the past. `X` is typically `Bundle` or `Resource`.
///
/// The `'tick` lifetime is invariant and prevents mixing resources the come from different times.
#[derive(Debug)]
#[repr(transparent)]
pub struct Past<'tick, X> {
    x: X,
    phantom: PhantomData<fn(&'tick ()) -> &'tick ()>,
}

impl<'tick, X> Past<'tick, X> {
    fn into_inner(self) -> X {
        self.x
    }
    fn as_inner_mut(&mut self) -> &mut X {
        // Safety: `Past` is `repr(transparent)`; the types are otherwise the same.
        unsafe { std::mem::transmute(self) }
    }
    fn from_inner(x: X) -> Self {
        Self {
            x,
            phantom: PhantomData,
        }
    }
    fn from_inner_mut(x: &mut X) -> &mut Self {
        // Safety: `Past` is `repr(transparent)`; the types are otherwise the same.
        unsafe { std::mem::transmute(x) }
    }
}

impl<'tick, X> Deref for Past<'tick, X> {
    type Target = X;
    fn deref(&self) -> &Self::Target {
        &self.x
    }
}

impl<'tick, R> Past<'tick, Resource<R>>
where
    R: ResourceType,
{
    /// Creates a new empty [`Resource`].
    pub fn new_empty() -> Self {
        Past::from_inner(Resource::new_empty())
    }

    /// Adds the entire Rs of another resource container to this one.
    /// You can also use `+=`.
    pub fn add(&mut self, other: impl Into<Self>) {
        self.as_inner_mut().add(other.into().x);
    }

    /// Takes a specified amount of resources from this [`Resource`] and puts it into a [`Bundle`].
    pub fn bundle<const AMOUNT: u32>(
        &mut self,
    ) -> Result<Past<'tick, Bundle<R, AMOUNT>>, InsufficientResourceError<R>> {
        let bundle = self.as_inner_mut().bundle()?;
        Ok(Past::from_inner(bundle))
    }

    /// Empties this [`Resource`], returning all contained resources as a new [`Resource`].
    pub fn empty(&mut self) -> Self {
        #[allow(clippy::mem_replace_with_default)] // doesn't work in `const`
        std::mem::replace(self, Self::new_empty())
    }

    /// Removes up to the specified amount of resources from this [`Resource`] and returns them as a new [`Resource`].
    /// If there are insufficient resources in the [`Resource`], it returns all available resources.
    pub fn split_off_max(&mut self, amount: u32) -> Self {
        Self::from_inner(self.as_inner_mut().split_off_max(amount))
    }

    /// Makes this resource available to the present.
    pub fn into_present(self) -> Resource<R> {
        self.x
    }
}

impl<'tick, R> AddAssign for Past<'tick, Resource<R>>
where
    R: ResourceType,
{
    fn add_assign(&mut self, rhs: Self) {
        *self.as_inner_mut() += rhs.into_inner()
    }
}

impl<'tick, R, const AMOUNT: u32> Past<'tick, Bundle<R, AMOUNT>>
where
    R: ResourceType,
{
    /// Makes this bundle available to the present.
    pub fn into_present(self) -> Bundle<R, AMOUNT> {
        self.x
    }
}

impl<'tick, R, const AMOUNT: u32> From<Past<'tick, Bundle<R, AMOUNT>>> for Past<'tick, Resource<R>>
where
    R: ResourceType,
{
    fn from(bundle: Past<'tick, Bundle<R, AMOUNT>>) -> Self {
        Self::from_inner(bundle.x.into())
    }
}

/// A helper for making reusable subfactories.
pub struct Subfactory<F: OnEachTick> {
    snapshot: TickSnapshot,
    f: F,
}

/// A struct containing machines that wants to be updated at every tick. Used in `Subfactory`.
pub trait OnEachTick {
    /// Called on every past tick whenever the factory is accessed.
    fn on_each_tick<'tick>(&mut self, tick: &PastTick<'tick>);
}

impl<F: OnEachTick> Subfactory<F> {
    /// Create a new subfactory.
    pub const fn new(tick: &Tick, f: F) -> Self {
        Self {
            snapshot: tick.snapshot(),
            f,
        }
    }

    fn tick(&mut self, tick: impl Into<TickSnapshot>) {
        self.snapshot
            .on_each_tick(tick.into(), |past_tick| {
                self.f.on_each_tick(past_tick);
            })
            .unwrap();
    }

    /// Access the contained machines.
    pub fn inner<'a>(&'a mut self, tick: &'a Tick) -> &'a mut F {
        self.tick(tick);
        &mut self.f
    }
    /// Retrieve the contained machines.
    pub fn into_inner(mut self, tick: &Tick) -> F {
        self.tick(tick);
        self.f
    }

    /// Access the contained machines in a way that can mutate the tick. This will not call
    /// `on_each_tick` for the ticks advanced thus, because that would access whichever machine
    /// updated the tick at a past time.
    ///
    /// As an alternative to this, store `Option`s of the machines you want to use in this way, so
    /// that you can move them out, use them to move time forward, then put them back.
    pub fn inner_tick_mut(
        &mut self,
        tick: &mut Tick,
        f: impl for<'a> FnOnce(&'a mut Tick, &'a mut F),
    ) {
        self.tick(&*tick);
        f(tick, &mut self.f);
        // Here if we called `tick`, the machine that got moved forward may be accessed in a past
        // moment by `F::on_each_tick`. So we skip without calling `on_each_tick`.
        self.snapshot = tick.snapshot();
    }

    /// Access the contained machines.
    // The `tick` is unused in the output which is fine, the contained machines will deal with
    // giving out `'tick`-bound resources.
    pub fn past_inner<'a, 'tick>(&'a mut self, tick: &'a PastTick<'tick>) -> &'a mut F {
        self.tick(tick);
        &mut self.f
    }
}

/// An item that can store resources across time. This should be rarely needed since the
/// inputs/outputs of machines already work like that, but this exists if needed.
/// Basically a machine with a recipe that does nothing.
#[derive(Debug)]
pub struct ResourceStore<R: ResourceType> {
    resource: Resource<R>,
    tick: TickSnapshot,
}

impl<R: ResourceType> ResourceStore<R> {
    /// Create a new resource store.
    pub fn new(tick: TickSnapshot) -> Self {
        Self {
            resource: Default::default(),
            tick,
        }
    }

    /// Access the contained resource in the present.
    pub fn resources<'a>(&'a mut self, tick: &'a Tick) -> &'a mut Resource<R> {
        self.tick_to(tick.snapshot()).unwrap();
        &mut self.resource
    }

    /// Access the contained resource in the past if the store has not yet been observed in the
    /// present.
    pub fn past_resource<'a, 'tick>(
        &'a mut self,
        tick: &'a PastTick<'tick>,
    ) -> Result<&'a mut Past<'tick, Resource<R>>, BackwardTickingError> {
        self.tick_to(tick.as_snapshot())?;
        Ok(Past::from_inner_mut(&mut self.resource))
    }

    fn tick_to(&mut self, until: TickSnapshot) -> Result<(), BackwardTickingError> {
        self.tick.advance_to(until)?;
        Ok(())
    }
}
