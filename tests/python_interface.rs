use std::process::Command;

#[test]
fn python_document_is_parseable_and_round_trips() {
    let script = r#"
from opencity_agent import LaneSplitMode, LandscapeKind, Point, RoadDirection, RoadDocument, RoadElement
d = RoadDocument()
d.define_road('one_way', [RoadElement.VERGE, RoadElement.CAR_LANE], RoadDirection.FORWARD)
d.point('a', Point(0, 0))
d.point('b', Point(2, 0))
d.road('one_way', 'a', 'b', name='road_1', control1=Point(0.5, 0), control2=Point(1.5, 0))
d.area(LandscapeKind.BEACH, [Point(-1, -1), Point(3, -1), Point(0, 2)], name='beach_1')
d.vegetation('trees', 'forest', [Point(0, 0), Point(1, 0), Point(0, 1)])
d.terrain('hill', 0.2, [Point(0, 0), Point(1, 0), Point(0, 1)])
d.building('hall', 0.5, [Point(0, 0), Point(1, 0), Point(0, 1)], model='assets/hall.glb')
d.grade('road_1', 0, 0.2)
d.lane_point('entry', 'road_1', 1, 0.5)
d.lane_split('road_1', 'road_1', LaneSplitMode.KEEP_LANE)
print(d.source)
"#;
    let output = Command::new("python3")
        .arg("-c")
        .arg(script)
        .env("PYTHONPATH", concat!(env!("CARGO_MANIFEST_DIR"), "/python"))
        .output()
        .expect("python3 is required for the Python interface test");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source = String::from_utf8(output.stdout).unwrap();
    let parsed = opencity::parse(&source).unwrap();
    assert_eq!(opencity::parse(&parsed.to_road_source()).unwrap(), parsed);
}
