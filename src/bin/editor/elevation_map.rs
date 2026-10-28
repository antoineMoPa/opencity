//! Shared elevation asset loading for scene construction and road placement.
use super::*;
use opencity::heightmap::{ElevationMap, ElevationMapSpec};

#[cfg(not(target_arch = "wasm32"))]
type CachedMap = (PathBuf, std::time::SystemTime, u64, Arc<ElevationMap>);

pub(super) fn load(
    program: &Program,
    base: Option<&Path>,
) -> Result<Option<Arc<ElevationMap>>, String> {
    let Some(spec) = ElevationMapSpec::from_program(program)? else {
        return Ok(None);
    };
    // An unsaved default document has no asset directory. Bundle its terrain
    // so startup works from any working directory, including the web editor.
    if base.is_none() && spec.path == "default_elevation.png" {
        static BUNDLED: std::sync::OnceLock<std::sync::Mutex<Option<Arc<ElevationMap>>>> =
            std::sync::OnceLock::new();
        let mut cache = BUNDLED.get_or_init(Default::default).lock().unwrap();
        if let Some(map) = cache.as_ref().filter(|map| map.spec == spec) {
            return Ok(Some(Arc::clone(map)));
        }
        let map = Arc::new(ElevationMap::from_png(
            spec,
            include_bytes!("../../../examples/default_elevation.png"),
        )?);
        *cache = Some(Arc::clone(&map));
        return Ok(Some(map));
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = base;
        let _ = spec;
        Err("External elevation PNG files require the desktop editor".into())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        static CACHE: std::sync::OnceLock<std::sync::Mutex<Option<CachedMap>>> =
            std::sync::OnceLock::new();
        let path = base.unwrap_or_else(|| Path::new(".")).join(&spec.path);
        let metadata = std::fs::metadata(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let modified = metadata.modified().map_err(|e| e.to_string())?;
        let mut cache = CACHE.get_or_init(Default::default).lock().unwrap();
        if let Some((cached_path, time, len, map)) = cache.as_ref() {
            if *cached_path == path
                && *time == modified
                && *len == metadata.len()
                && map.spec == spec
            {
                return Ok(Some(Arc::clone(map)));
            }
        }
        let map = Arc::new(spec.load(base)?);
        *cache = Some((path, modified, metadata.len(), Arc::clone(&map)));
        Ok(Some(map))
    }
}

pub(super) fn pick(
    map: &ElevationMap,
    at_height: &impl Fn(f32) -> WorldPoint,
) -> Option<(WorldPoint, f32)> {
    let terrain = pick_terrain(map, at_height);
    if let Some(level) = map.spec.water_level {
        let p = at_height(level);
        if map.height_at([p.x, p.y]).is_some_and(|z| z < level)
            && terrain.is_none_or(|(_, height)| level > height)
        {
            return Some((p, level));
        }
    }
    terrain
}

fn pick_terrain(
    map: &ElevationMap,
    at_height: &impl Fn(f32) -> WorldPoint,
) -> Option<(WorldPoint, f32)> {
    let low = map.spec.heights[0];
    let high = map.spec.heights[1];
    let a = at_height(high);
    let b = at_height(low);
    let pixel = |p: WorldPoint| {
        [
            (p.x - map.spec.origin[0]) / map.spec.size[0] * 1023.0,
            (p.y - map.spec.origin[1]) / map.spec.size[1] * 1023.0,
        ]
    };
    let p = pixel(a);
    let q = pixel(b);
    let mut cuts = vec![0.0_f32, 1.0];
    // Cell edges and diagonals partition the ray into linear surface intervals.
    for (start, end, first, last) in [
        (p[0], q[0], 0, 1023),
        (p[1], q[1], 0, 1023),
        (p[0] - p[1], q[0] - q[1], -1023, 1023),
    ] {
        if (end - start).abs() > f32::EPSILON {
            for edge in first..=last {
                let t = (edge as f32 - start) / (end - start);
                if t > 0.0 && t < 1.0 {
                    cuts.push(t);
                }
            }
        }
    }
    cuts.sort_by(f32::total_cmp);
    for pair in cuts.windows(2) {
        let height = |t| high + (low - high) * t;
        let sample = |t| {
            let p = at_height(height(t));
            map.height_at([p.x, p.y]).map(|z| z - height(t))
        };
        let (Some(d0), Some(d1)) = (sample(pair[0]), sample(pair[1])) else {
            continue;
        };
        if d0.abs() < 1e-5 {
            let z = height(pair[0]);
            return Some((at_height(z), z));
        }
        if d0 <= 0.0 && d1 >= 0.0 {
            let t = pair[0] + (pair[1] - pair[0]) * (-d0) / (d1 - d0);
            let z = height(t);
            return Some((at_height(z), z));
        }
    }
    // Constant-height maps also have a single surface.
    let p = at_height(low);
    (low == high && map.height_at([p.x, p.y]).is_some()).then_some((p, low))
}

#[cfg(not(target_arch = "wasm32"))]
impl RoadEditor {
    pub(super) fn import_elevation_map(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("1024×1024 grayscale elevation PNG", &["png"])
            .pick_file()
        else {
            return;
        };
        let source = self
            .path
            .as_deref()
            .and_then(Path::parent)
            .and_then(|base| path.strip_prefix(base).ok())
            .unwrap_or(&path);
        let call = Call {
            assigns: Vec::new(),
            name: "elevation_map".into(),
            args: std::iter::once(Argument::String(format!(
                "\"{}\"",
                source
                    .to_string_lossy()
                    .replace('\\', "\\\\")
                    .replace('"', "\\\"")
            )))
            .chain(
                ["-10", "-10", "20", "20", "0", "2"]
                    .into_iter()
                    .map(|v| Argument::Number(v.into())),
            )
            .collect(),
        };
        let mut draft = self.program.clone();
        draft
            .statements
            .retain(|s| !matches!(s, Statement::Call(c) if c.name == "elevation_map"));
        draft.statements.push(Statement::Call(call));
        match load(&draft, self.path.as_deref().and_then(Path::parent)) {
            Ok(_) => {
                self.cancel_placement();
                self.program = draft;
                self.scene_cache = None;
                self.scene_build = None;
                self.fit_scene_once = true;
                self.status = "Imported elevation map: 20×20 map units, heights 0–2; edit \
                    elevation_map bounds in the .road file"
                    .into();
            }
            Err(error) => self.status = format!("Could not import elevation map: {error}"),
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    pub(crate) struct Fixture {
        pub dir: PathBuf,
    }
    impl Fixture {
        pub(crate) fn new(pixel: impl Fn(usize, usize) -> u8) -> Self {
            static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            let dir = std::env::temp_dir().join(format!(
                "opencity-heightmap-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&dir).unwrap();
            let mut data = Vec::with_capacity(1024 * 1024);
            for y in 0..1024 {
                for x in 0..1024 {
                    data.push(pixel(x, y));
                }
            }
            let file = std::fs::File::create(dir.join("height.png")).unwrap();
            let mut encoder = png::Encoder::new(file, 1024, 1024);
            encoder.set_color(png::ColorType::Grayscale);
            encoder.set_depth(png::BitDepth::Eight);
            encoder
                .write_header()
                .unwrap()
                .write_image_data(&data)
                .unwrap();
            Self { dir }
        }
        pub(crate) fn program(&self) -> Program {
            parse("road street: carlane; end elevation_map(\"height.png\", -2,-2,4,4,0,1);")
                .unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    #[test]
    fn caches_relative_assets_and_keeps_mapping_independent() {
        let fixture = Fixture::new(|_, _| 255);
        let program = fixture.program();
        let a = load(&program, Some(&fixture.dir)).unwrap().unwrap();
        let b = load(&program, Some(&fixture.dir)).unwrap().unwrap();
        assert_eq!(a.spec, b.spec);
        assert_eq!(a.height_at([0.0, 0.0]), Some(1.0));
        assert!(load(&program, Some(&fixture.dir.join("missing"))).is_err());
        let mut changed = program.clone();
        let Statement::Call(call) = changed.statements.last_mut().unwrap() else {
            panic!()
        };
        call.args[6] = Argument::Number("2".into());
        assert_eq!(
            load(&changed, Some(&fixture.dir))
                .unwrap()
                .unwrap()
                .height_at([0.0, 0.0]),
            Some(2.0)
        );
    }
    #[test]
    fn ray_picking_finds_slopes_and_negative_elevations() {
        let fixture = Fixture::new(|x, _| (x * 255 / 1023) as u8);
        let map = load(&fixture.program(), Some(&fixture.dir))
            .unwrap()
            .unwrap();
        let (p, z) = pick(&map, &|z| WorldPoint { x: z - 0.5, y: 0.0 }).unwrap();
        assert!((map.height_at([p.x, p.y]).unwrap() - z).abs() < 1e-5);
        let fixture = Fixture::new(|_, _| 0);
        let mut spec = opencity::heightmap::ElevationMapSpec::from_program(&fixture.program())
            .unwrap()
            .unwrap();
        spec.heights = [-2.0, -1.0];
        let map = spec.load(Some(&fixture.dir)).unwrap();
        assert_eq!(
            pick(&map, &|_| WorldPoint { x: 0.0, y: 0.0 }).unwrap().1,
            -2.0
        );
    }
    #[test]
    fn mesh_keeps_height_samples_and_merges_flat_regions() {
        let fixture = Fixture::new(|_, _| 255);
        let map = load(&fixture.program(), Some(&fixture.dir))
            .unwrap()
            .unwrap();
        let palette = editor_palette(egui::Theme::Light);
        let scene = scene3d::build_scene_with_model_base(
            &fixture.program(),
            &map::project(&fixture.program()),
            palette,
            &[],
            None,
            Some(&fixture.dir),
        );
        assert_eq!(scene.vertices.len(), 6);
        assert!(scene.vertices.iter().all(|v| v.height == 1.0));
        assert_eq!(map.point(1023, 1023), [2.0, 2.0, 1.0]);
    }
}

#[cfg(test)]
mod export_tests {
    use super::*;
    #[test]
    fn png_terrain_survives_glb_export_with_relative_asset_path() {
        let fixture = tests::Fixture::new(|_, _| 255);
        let bytes = glb::export_with_base(&fixture.program(), Some(&fixture.dir));
        let model = model::parse_glb(&bytes).unwrap();
        assert_eq!(model.triangles.len(), 2);
        assert_eq!(model.min, [-2.0, 1.0, -2.0]);
        assert_eq!(model.max, [2.0, 1.0, 2.0]);
    }
    #[test]
    fn bundled_heightmap_example_loads_and_retains_its_hills() {
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples");
        let program = parse(include_str!("../../../examples/elevation_map.road")).unwrap();
        let map = load(&program, Some(&base)).unwrap().unwrap();
        let mut count = 0;
        let mut highest = 0.0_f32;
        map.for_each_triangle(|triangle| {
            count += 1;
            for point in triangle {
                highest = highest.max(point[2]);
            }
        });
        assert!(highest > 1.5);
        assert!(
            count > 100_000 && count <= 2_093_058,
            "noise island should retain its full-resolution detail: {count}"
        );
        assert_eq!(map.spec.water_level, Some(0.35));
        for i in 0..1024 {
            for point in [
                map.point(i, 0),
                map.point(i, 1023),
                map.point(0, i),
                map.point(1023, i),
            ] {
                assert_eq!(point[2], 0.0, "island edges must be black lowlands");
            }
        }
        assert!(map.surface_height_at([-10.0, -10.0]).unwrap() == 0.35);
        assert_eq!(map::project(&program).unresolved, 0);
    }
}

#[cfg(test)]
mod water_export_tests {
    use super::*;
    #[test]
    fn glb_includes_the_water_plane_and_preserves_the_underwater_bed() {
        let fixture = tests::Fixture::new(|_, _| 0);
        let mut program = fixture.program();
        program
            .statements
            .extend(parse("water_level(0.35);").unwrap().statements);
        let model = model::parse_glb(&glb::export_with_base(&program, Some(&fixture.dir))).unwrap();
        assert_eq!(model.triangles.len(), 4);
        assert_eq!(model.min[1], 0.0);
        assert_eq!(model.max[1], 0.35);
        let scene = scene3d::build_scene_with_model_base(
            &program,
            &map::project(&program),
            editor_palette(egui::Theme::Light),
            &[],
            None,
            Some(&fixture.dir),
        );
        assert_eq!(
            scene.vertices.len(),
            6,
            "submerged bed must not show green map-edge strips"
        );
        assert!(scene.vertices.iter().all(|vertex| vertex.height == 0.35));
    }
}
