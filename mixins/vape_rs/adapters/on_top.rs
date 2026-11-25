fn on_top(
    lines: &[Line],
    camera: [f64; 3],
    view: glam::Mat4,
) -> core::ops::ControlFlow<Vec<[f32; 8]>> {
    horizon_client_lib::gizmos::vape_on_top::camera_relative(lines, camera, view)
}
