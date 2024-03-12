use std::{fmt::Debug, mem};
use bevy_reflect::{std_traits::ReflectDefault, Reflect};
use downcast_rs::{impl_downcast, Downcast};

const _: Option<Box<dyn DataTransfer>> = None;

/// A type that can be boxed into a dynamic object.
/// 
/// The trait bound [`Clone`], [`Debug`] and [`PartialEq`] are required for maximum usability.
pub trait DataTransfer: Downcast + Debug + Send + Sync + 'static {
    fn dyn_clone(&self) -> Box<dyn DataTransfer>;
    fn dyn_eq(&self, other: &dyn DataTransfer) -> bool;
}

impl_downcast!(DataTransfer);

impl<T> DataTransfer for T where T: Debug + Clone + PartialEq + Send + Sync + 'static{
    fn dyn_clone(&self) -> Box<dyn DataTransfer> {
        Box::new(self.clone())
    }

    fn dyn_eq(&self, other: &dyn DataTransfer) -> bool {
        match other.downcast_ref::<T>() {
            Some(some) => some == self,
            None => false,
        }
    }
}

/// A type that can converted to and from [`Object`].
pub trait AsObject: Sized + Debug + Clone + Send + Sync + 'static {
    fn get(obj: &Object) -> Option<Self>;
    fn get_ref(obj: &Object) -> Option<&Self>;
    fn get_mut(obj: &mut Object) -> Option<&mut Self>;
    fn from_object(obj: Object) -> Option<Self>;
    fn into_object(self) -> Object;
    fn as_dyn_inner(&self) -> Option<&dyn DataTransfer>;
}

impl<T> AsObject for T where T: DataTransfer + Clone {
    fn get(obj: &Object) -> Option<Self> {
        obj.0.as_ref().and_then(|x| x.dyn_clone().downcast().ok().map(|x| *x))
    }

    fn get_ref(obj: &Object) -> Option<&Self> {
        obj.0.as_ref().and_then(|x| x.downcast_ref())
    }
    
    fn get_mut(obj: &mut Object) -> Option<&mut Self> {
        obj.0.as_mut().and_then(|x| x.downcast_mut())
    }
    
    fn from_object(obj: Object) -> Option<Self> {
        obj.0.and_then(|x| x.downcast().map(|x| *x).ok())
    }

    fn into_object(self) -> Object {
        Object(Some(Box::new(self)))
    }

    fn as_dyn_inner(&self) -> Option<&dyn DataTransfer> {
        Some(self)
    }
}

impl AsObject for Object  {
    fn get(obj: &Object) -> Option<Self> {
        if obj.is_some(){
            Some(obj.clone())
        } else {
            None
        }
    }

    fn get_ref(obj: &Object) -> Option<&Self> {
        if obj.is_some(){
            Some(obj)
        } else {
            None
        }
    }

    fn get_mut(obj: &mut Object) -> Option<&mut Self> {
        if obj.is_some(){
            Some(obj)
        } else {
            None
        }
    }

    fn from_object(obj: Object) -> Option<Self> {
        if obj.is_some(){
            Some(obj)
        } else {
            None
        }
    }

    fn into_object(self) -> Object {
        self
    }
    
    fn as_dyn_inner(&self) -> Option<&dyn DataTransfer> {
        self.0.as_ref().map(|x| x.as_ref())
    }
}

/// A boxed type erased nullable dynamic object.
/// 
/// # Note
/// 
/// Object is special handled from the blanket implementations in order to prevent
/// the situation of boxing an object into another object.
/// Without specialization, we choose to not implement `PartialEq`, which causes the
/// least usability issues. Use `equal_to` instead.
#[derive(Debug)]
#[derive(Default, Reflect)]
#[reflect(Default)]
pub struct Object(#[reflect(ignore)] Option<Box<dyn DataTransfer>>);

impl Clone for Object {
    fn clone(&self) -> Self {
        Self(self.0.as_ref().map(|x| x.dyn_clone()))
    }
}

impl Object {
    /// A `None` object, if sent through a signal, does nothing.
    pub const NONE: Self = Self(None);

    /// Create an unnameable object that is never equal to anything.
    pub fn unnameable() -> Self {
        #[derive(Debug, Clone)]
        struct UnnameableUnequal;

        impl PartialEq for UnnameableUnequal{
            fn eq(&self, _: &Self) -> bool {
                false
            }
        }
        Self(Some(Box::new(UnnameableUnequal)))
    }

    /// Create a new object from a value.
    pub fn new<T: AsObject>(v: T) -> Self {
        AsObject::into_object(v)
    }

    /// Return true if object is not `None`.
    pub fn is_some(&self) -> bool {
        self.0.is_some()
    }

    /// Return true if object is `None`.
    pub fn is_none(&self) -> bool {
        self.0.is_none()
    }

    /// Try obtain a value by cloning.
    pub fn get<T: AsObject>(&self) -> Option<T> {
        AsObject::get(self)
    }

    /// Try obtain the value's reference.
    pub fn get_ref<T: AsObject>(&self) -> Option<&T> {
        AsObject::get_ref(self)
    }

    /// Try obtain the value's mutable reference.
    pub fn get_mut<T: AsObject>(&mut self) -> Option<&mut T> {
        AsObject::get_mut(self)
    }

    /// Remove the value from the object, leaving behind a `Object::NONE`.
    pub fn clean(&mut self) {
        self.0.take();
    }

    /// Take the value from the object, leaving behind a `Object::NONE`.
    pub fn take<T: AsObject>(&mut self) -> Option<T> {
        AsObject::from_object(mem::take(self))
    }

    /// Set the value of the object.
    pub fn set<T: AsObject>(&mut self, v: T) {
        *self = AsObject::into_object(v)
    }

    /// Swap the value of the object with another value.
    pub fn swap<T: AsObject>(&mut self, v: T) -> Option<T>{
        let result = self.take();
        *self = AsObject::into_object(v);
        result
    }

    /// Compare Object to a value that can be converted to an object.
    pub fn equal_to<T: AsObject>(&self, other: &T) -> bool {
        match (self.as_dyn_inner(), other.as_dyn_inner())  {
            (None, None) => true,
            (Some(a), Some(b)) => a.dyn_eq(b),
            _ => false
        }
    }

    /// If none, box another value as a new object.
    pub fn or<T: AsObject>(self, item: T) -> Object {
        if self.is_none() {
            Object::new(item)
        } else {
            self
        }
    }

    /// If none, box another value as a new object.
    pub fn or_else<T: AsObject>(self, item: impl Fn() -> T) -> Object {
        if self.is_none() {
            Object::new(item())
        } else {
            self
        }
    }
}
