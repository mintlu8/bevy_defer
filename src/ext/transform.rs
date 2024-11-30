use bevy::math::{Dir3, Quat, Vec3};
use bevy::transform::prelude::Transform;
use ref_cast::RefCast;

use crate::{
    access::{deref::AsyncComponentDeref, AsyncComponent},
    AccessResult, AsyncAccess,
};

#[derive(Debug, RefCast)]
#[repr(transparent)]
pub struct AsyncTransform(AsyncComponent<Transform>);

impl AsyncComponentDeref for Transform {
    type Target = AsyncTransform;

    fn async_deref(this: &AsyncComponent<Self>) -> &Self::Target {
        AsyncTransform::ref_cast(this)
    }
}

impl AsyncTransform {
    pub fn translation(&self) -> AccessResult<Vec3> {
        self.0.get(|x| x.translation)
    }

    pub fn rotation(&self) -> AccessResult<Quat> {
        self.0.get(|x| x.rotation)
    }

    pub fn scale(&self) -> AccessResult<Vec3> {
        self.0.get(|x| x.scale)
    }

    pub fn set_translation(&self, translation: Vec3) -> AccessResult {
        self.0.get_mut(|x| x.translation = translation)
    }

    pub fn set_rotation(&self, rotation: Quat) -> AccessResult {
        self.0.get_mut(|x| x.rotation = rotation)
    }

    pub fn set_scale(&self, scale: Vec3) -> AccessResult {
        self.0.get_mut(|x| x.scale = scale)
    }

    pub fn forward(&self) -> AccessResult<Dir3> {
        self.0.get(|x| x.forward())
    }

    pub fn back(&self) -> AccessResult<Dir3> {
        self.0.get(|x| x.back())
    }

    pub fn up(&self) -> AccessResult<Dir3> {
        self.0.get(|x| x.up())
    }

    pub fn down(&self) -> AccessResult<Dir3> {
        self.0.get(|x| x.down())
    }

    pub fn left(&self) -> AccessResult<Dir3> {
        self.0.get(|x| x.left())
    }

    pub fn right(&self) -> AccessResult<Dir3> {
        self.0.get(|x| x.right())
    }

    pub fn look_at(&self, target: Vec3, up: impl TryInto<Dir3>) -> AccessResult {
        self.0.get_mut(|x| x.look_at(target, up))
    }

    pub fn look_to(&self, direction: impl TryInto<Dir3>, up: impl TryInto<Dir3>) -> AccessResult {
        self.0.get_mut(|x| x.look_to(direction, up))
    }

    pub fn translate_by(&self, translation: Vec3) -> AccessResult {
        self.0.get_mut(|x| x.translation += translation)
    }

    pub fn rotate_by(&self, rotation: Quat) -> AccessResult {
        self.0.get_mut(|x| x.rotate(rotation))
    }

    pub fn scale_by(&self, scale: Vec3) -> AccessResult {
        self.0.get_mut(|x| x.scale *= scale)
    }

    pub fn rotate_around(&self, point: Vec3, rotation: Quat) -> AccessResult {
        self.0.get_mut(|x| x.rotate_around(point, rotation))
    }

    pub fn rotate_local_axis(&self, axis: Dir3, value: f32) -> AccessResult {
        self.0.get_mut(|x| x.rotate_local_axis(axis, value))
    }

    pub fn rotate_x_by(&self, value: f32) -> AccessResult {
        self.0.get_mut(|x| x.rotate_x(value))
    }

    pub fn rotate_y_by(&self, value: f32) -> AccessResult {
        self.0.get_mut(|x| x.rotate_y(value))
    }

    pub fn rotate_z_by(&self, value: f32) -> AccessResult {
        self.0.get_mut(|x| x.rotate_z(value))
    }

    pub fn rotate_local_x_by(&self, value: f32) -> AccessResult {
        self.0.get_mut(|x| x.rotate_local_x(value))
    }

    pub fn rotate_local_y_by(&self, value: f32) -> AccessResult {
        self.0.get_mut(|x| x.rotate_local_y(value))
    }

    pub fn rotate_local_z_by(&self, value: f32) -> AccessResult {
        self.0.get_mut(|x| x.rotate_local_z(value))
    }
}
