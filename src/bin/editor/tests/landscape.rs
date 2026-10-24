use super::*;

#[test]
fn convex_landscape_outline_uses_a_single_fill() {
    let mut rectangle = vec![
        egui::pos2(0.0, 0.0),
        egui::pos2(4.0, 0.0),
        egui::pos2(4.0, 3.0),
        egui::pos2(0.0, 3.0),
    ];
    assert!(landscape_is_convex(&rectangle));
    rectangle.reverse();
    assert!(landscape_is_convex(&rectangle));
    rectangle.insert(4, egui::pos2(0.0, 1.5));
    assert!(landscape_is_convex(&rectangle));
    assert!(!landscape_is_convex(&rectangle[..2]));
}
