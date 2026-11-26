fn send_state_ground(value: S) -> S {
    let ground = |flags: u8| {
        let Some(ground) = crate::game::remote::vape_movement::take_ground() else {
            return flags;
        };
        (flags & !1) | u8::from(ground)
    };
    let rotation = |yaw: f32, pitch: f32| {
        crate::game::remote::vape_combat_remote::rotation((yaw, pitch))
    };
    match value {
        S::MovePlayerPos(mut packet) => {
            packet.flags = ground(packet.flags);
            S::MovePlayerPos(packet)
        }
        S::MovePlayerPosRot(mut packet) => {
            packet.flags = ground(packet.flags);
            (packet.yaw, packet.pitch) = rotation(packet.yaw, packet.pitch);
            S::MovePlayerPosRot(packet)
        }
        S::MovePlayerRot(mut packet) => {
            packet.flags = ground(packet.flags);
            (packet.yaw, packet.pitch) = rotation(packet.yaw, packet.pitch);
            S::MovePlayerRot(packet)
        }
        other => other,
    }
}
