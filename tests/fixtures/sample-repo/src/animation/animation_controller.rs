use super::weapon_socket::WeaponSocket;

/// Updates the animation of one frame.
pub fn update_animation(socket: &mut WeaponSocket) {
    socket.attach_to_bone("hand_r");
}
