use futures_core::Future;
use std::fmt::Debug;
use std::{marker::PhantomData, pin::Pin};

use crate::{ValueInner, ValueIsReadOnly};

#[derive(Debug)]
pub struct MappedReadonlyValue<T: Clone, V: ValueInner<T>, U, F: Fn(T) -> U> {
    pub(crate) value: V,
    pub(crate) mapper: F,
    pub(crate) p: PhantomData<(T, U)>,
}

impl<U: Clone + Send + Sync, T: Send + Sync, V: ValueInner<U>, F: Fn(U) -> T + Send + Sync>
    ValueInner<T> for MappedReadonlyValue<U, V, T, F>
{
    fn as_debug(&self) -> &dyn Debug {
        #[derive(Debug)]
        pub struct MapperValueInner;
        &MapperValueInner
    }
    fn read_tick(&self) -> u32 {
        self.value.read_tick()
    }

    fn write(&self, _: T) -> Result<u32, ValueIsReadOnly> {
        Err(ValueIsReadOnly)
    }

    fn write_if_changed(&self, _: T) -> Result<u32, ValueIsReadOnly>
    where
        T: PartialEq,
    {
        Err(ValueIsReadOnly)
    }

    fn read_async<'t>(&'t self, tick: u32) -> Pin<Box<dyn Future<Output = (T, u32)> + 't>>
    where
        T: Clone,
    {
        Box::pin(async move {
            let (value, tick) = self.value.read_async(tick).await;
            ((self.mapper)(value), tick)
        })
    }

    fn read(&self, tick: u32) -> Option<(T, u32)>
    where
        T: Clone,
    {
        self.value
            .read(tick)
            .map(|(value, tick)| ((self.mapper)(value), tick))
    }

    fn force_read(&self) -> Option<(T, u32)>
    where
        T: Clone,
    {
        self.value
            .force_read()
            .map(|(value, tick)| ((self.mapper)(value), tick))
    }
}

#[derive(Debug)]
pub struct MappedRwValue<T: Clone, V: ValueInner<T>, U, F1: Fn(T) -> U, F2: Fn(U) -> T> {
    pub(crate) value: V,
    pub(crate) w2r: F1,
    pub(crate) r2w: F2,
    pub(crate) p: PhantomData<(T, U)>,
}

impl<
        U: Clone + Send + Sync,
        T: Send + Sync,
        V: ValueInner<U>,
        F1: Fn(U) -> T + Send + Sync,
        F2: Fn(T) -> U + Send + Sync,
    > ValueInner<T> for MappedRwValue<U, V, T, F1, F2>
{
    fn as_debug(&self) -> &dyn Debug {
        #[derive(Debug)]
        pub struct MapperValueInner;
        &MapperValueInner
    }
    fn read_tick(&self) -> u32 {
        self.value.read_tick()
    }

    fn write(&self, item: T) -> Result<u32, ValueIsReadOnly> {
        self.value.write((self.r2w)(item))
    }

    fn write_if_changed(&self, item: T) -> Result<u32, ValueIsReadOnly>
    where
        T: PartialEq,
    {
        self.value.write((self.r2w)(item))
    }

    fn read_async<'t>(&'t self, tick: u32) -> Pin<Box<dyn Future<Output = (T, u32)> + 't>>
    where
        T: Clone,
    {
        Box::pin(async move {
            let (value, tick) = self.value.read_async(tick).await;
            ((self.w2r)(value), tick)
        })
    }

    fn read(&self, tick: u32) -> Option<(T, u32)>
    where
        T: Clone,
    {
        self.value
            .read(tick)
            .map(|(value, tick)| ((self.w2r)(value), tick))
    }

    fn force_read(&self) -> Option<(T, u32)>
    where
        T: Clone,
    {
        self.value
            .force_read()
            .map(|(value, tick)| ((self.w2r)(value), tick))
    }
}
