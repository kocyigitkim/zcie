/// A socket that holds a weapon on a bone.
pub struct WeaponSocket {
    pub bone: String,
}

impl WeaponSocket {
    /// Attaches the weapon to a bone.
    pub fn attach_to_bone(&mut self, bone: &str) {
        self.bone = bone.to_string();
    }
}
