fn send_state_ground(value: S) -> S {
    let Some(ground) = crate::game::remote::vape_movement::take_ground() else {
        return value;
    };
    let flags = |flags: u8| (flags & !1) | u8::from(ground);
    match value {
        S::MovePlayerPos(mut packet) => {
            packet.flags = flags(packet.flags);
            S::MovePlayerPos(packet)
        }
        S::MovePlayerPosRot(mut packet) => {
            packet.flags = flags(packet.flags);
            S::MovePlayerPosRot(packet)
        }
        S::MovePlayerRot(mut packet) => {
            packet.flags = flags(packet.flags);
            S::MovePlayerRot(packet)
        }
        S::MovePlayerStatus(mut packet) => {
            packet.flags = flags(packet.flags);
            S::MovePlayerStatus(packet)
        }
        other => other,
    }
}
