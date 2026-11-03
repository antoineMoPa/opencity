# Selective geographic lookup for agent-authored maps

The CLI reads **Overpass JSON** or **PBF node/way/relation data**, emits **GeoJSON
in WGS84**, and separately maps coordinates into an authored world frame. It
contains no city-specific calibration, dataset location, provider endpoint or
provider attribution. The agent decides which source features to keep and how
much to simplify; lookup never edits `.road` files or joins features.

## Build and sources

The `geodata` feature is included by default. For a headless build:

```sh
cargo build --no-default-features --features geodata --bin opencity
```

Choose exactly one input source:

- `--pbf /path/to/region.pbf`: read a local extract without network access.
- `--input /path/to/response.json`: replay a saved Overpass JSON response.
- `--endpoint URL`: query an explicit Overpass-compatible endpoint using `curl`.
  `OPENCITY_OVERPASS_ENDPOINT` supplies the same setting through the environment.

There is no default network provider and no fallback to another endpoint. Local
inputs ignore the endpoint environment variable and reject an explicit
`--endpoint`. Network requests have bounded timeouts and a 16 MiB response cap.
PBF means the node/way/relation protocol, not an arbitrary protobuf payload;
its required header identifiers and decoder types retain their protocol names.

The Cargo feature was renamed from `osm` to `geodata`, and the endpoint environment
variable from `OPENCITY_OSM_ENDPOINT` to `OPENCITY_OVERPASS_ENDPOINT`. Update build
commands and launch configuration accordingly. Subcommands and output identity
fields remain compatible; `osm_id` and `osm_type` describe protocol object identity.

## Streets, places and addresses

These examples use illustrative coordinates and names, not a bundled city map:

```sh
target/debug/opencity fetch-street --pbf /path/to/region.pbf \
  --name "Canal Road" --bbox "0,0,1,1" \
  --output /tmp/street.geojson --raw-output /tmp/street-source.json
target/debug/opencity lookup-place --pbf /path/to/region.pbf \
  --name "Sample Building" --bbox "0,0,1,1" --output /tmp/places.geojson
target/debug/opencity lookup-place --input /tmp/place-source.json \
  --street "Canal Road" --number "42" --bbox "0,0,1,1"
```

Crop order is west, south, east, north, in WGS84 decimal degrees; the area cannot
exceed one square degree. Names are literal fragments with ASCII case-insensitive
comparison. Accents and hyphens remain unchanged. The query searches `name`,
`name:fr`, and `alt_name`; street queries additionally require a `highway` tag.
Addresses use explicit `addr:street` and exact `addr:housenumber` matching.

All in-crop vertices remain, including collinear vertices. Only crop-boundary
intersections are inserted, with null source node IDs. Outside excursions split
ways into separate features. Ways, carriageways and branches remain separate.
Matching paths remain candidates for the agent to assess. No matches returns an
empty collection rather than establishing global absence.

Place nodes retain coordinates. Ways and relations use bounding-box centers,
not surveyed entrances or footprint centroids. Local PBF reads resolve full
ordered way nodes and relation dependencies. Incomplete dependencies or truncated
frames fail before output writes. The extract stays unchanged and outside Git.

`--raw-output` saves replayable JSON. For PBF this contains resolved matching
objects rather than a copy of the binary file. Provenance records local path and
size; file modification time is not claimed as a dataset snapshot timestamp.
`--output` is optional; otherwise GeoJSON is printed to stdout. Input, output,
raw output and metadata files must be distinct.

## Source metadata and attribution

The protocol does not establish the provider or license of every dataset. Supply
its actual attribution, license and optional object URL pattern in a JSON sidecar:

```json
{
  "attribution": "Dataset contributors",
  "license": "https://data.example/license",
  "object_url_template": "https://data.example/{type}/{id}"
}
```

Pass `--source-metadata /path/to/source.json` with any input source. These are
illustrative values; use the dataset's own terms. The complete metadata object is
preserved in output and raw replay. `{type}` and `{id}` are substituted for source
object links. Existing element `source_url` values remain when no template is set.
Without a sidecar, replay metadata under `opencity_source_metadata` is retained,
and a response's `osm3s.copyright` is carried through as attribution. Unknown
licenses remain null; no provider or license is invented for local files.

## Coordinate calibration

After choosing candidate features, use an external map calibration:

```sh
target/debug/opencity map-coordinates --calibration /path/to/georeference.json \
  --input /tmp/street.geojson --output /tmp/street-world.json
target/debug/opencity map-coordinates --calibration /path/to/georeference.json \
  --lonlat "0.005,0.005"
target/debug/opencity map-coordinates --calibration /path/to/georeference.json \
  --world "1,2"
target/debug/opencity map-coordinates --calibration /path/to/georeference.json --report
```

`OpenCityMappedFeatures` preserves the original collection in `source_geojson`.
Each mapped feature retains its `source_feature_index`, vertex order and separate
geometry; source IDs/tags remain unchanged. World geometry is not geographic
GeoJSON. No heights, building orientation or reduced road geometry are inferred.

Calibration schema `opencity-georeference-v1` uses `local_web_mercator`, an
`origin_lonlat`, ordered `world_bounds`, and at least three non-collinear
`fit_controls` containing `id`, `lonlat`, and `world`. Optional `check_controls`
report held-out residuals separately. Frame-definition controls measure numerical
consistency, not surveyed accuracy. Game units need not be physical metres.

Optional `north_x_adjustment: [threshold_y, x_per_y]` explicitly represents an
existing regional correction: add `x_per_y * max(0, threshold_y - world_y)` to X.
The inverse removes it before reversing the affine fit. The CLI never estimates
or applies it implicitly. Unknown schemas, singular fits and invalid coordinates
fail before outputs are touched. Terrain is not warped by coordinate mapping.
