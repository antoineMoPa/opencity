# Pavement and material lighting

The `render-materials` feature exposes `runtime::surface_materials::SurfaceTextures`
and `EnvironmentMap`. Both the editor and game use these resources. CPU-only map
consumers can keep `game-runtime` without adding a GPU dependency.

## Surface inputs

Two mipmapped 1024² arrays hold the asphalt and concrete scans: sRGB albedo, and
linear RGB OpenGL normal + alpha roughness. Layers are explicitly asphalt=0 and
concrete=1. Their nominal physical tile widths are 3 m and 2 m. Consumers pass position
and derivatives in metres to `scanned_pavement`; the included editor/game shaders
use 100 metres per map unit. World projection handles slopes and vertical curbs;
it does not alter the geometry or UV metadata used for contact deposits and joints.
Green cycling and red/ochre transit palette colors are retained. Albedo variation
and normal strength fade over 2.5–20 cm per pixel toward measured scan averages,
preserving close material detail without distant repeated patches or sparkle.

### Seamless variation across scales

Fine detail blends three stochastic patches on a triangular lattice. Each shared
lattice vertex has a deterministic offset, quarter-turn, and scale in 0.86–1.14.
Cubed barycentric weights keep a patch dominant while giving continuous transitions
across every triangle edge. The same transforms and weights apply to albedo, normal,
and roughness. Sampled normal XY is rotated back into the surface frame; gradients
are transformed explicitly rather than inferred from discontinuous random offsets.

A second color layer samples the scan at 0.071× UV scale (about 28 m for concrete,
42 m for asphalt). It is low-pass filtered to avoid enlarged stones, then combined
with smooth, world-fixed macro variation. The broad layer survives distant detail
fading, so the material does not become a featureless average. Contrast compensation
around measured scan means reduces washed-out patch junctions; this is an approximate
variance correction, not full histogram-preserving synthesis.

For barycentric coordinates `b`, scan mean `mu`, and three patch samples `C_i`:

```text
w_i = b_i^3 / sum(b_j^3)
C_fine = max(0, mu + (sum(w_i * C_i) - mu) / sqrt(sum(w_i^2)))
C_surface = mix(mu, C_fine, resolved_detail) * coarse_variation
```

Each patch uses `textureSampleGrad` with its rotated/scaled original gradients.
Color, packed normal/roughness and their blend weights share one patch identity.

The nearby path uses seven logical texture samples: three albedo, three packed
normal/roughness, and one coarse color sample. Beyond 20 cm/pixel it smoothly reaches
a one-sample path using measured material means plus broad variation. No added
textures, draw calls, per-frame CPU work, or temporal randomness. Actual GPU cost
depends on resolution, anisotropic filtering and visible pavement coverage.

Technique background: [triangular stochastic tiling and
blending](https://unity-grenoble.github.io/website/publication/2019/06/01/publication-stochastic-texturing.html)
and [practical hexagonal tiling](https://jcgt.org/published/0011/03/05/paper.pdf).

Game bindings: group 1, bindings 8/9/10. Editor bindings: group 2, bindings 3/4/5.
Call the shared `layout(first)` and `entries(first)` helpers. Upload once per device;
the decoded, color-correct mip chains are cached across renderer instances.

## Environment and display

`EnvironmentMap` exposes a layout/group containing the sharp RGBA16F sky cube at
binding 0, linear mip sampler at 1, and filtered material-lighting cube at 2.
Game uses group 2; editor uses group 3. `bake` accepts sun XYZ/intensity,
sun RGB/turbidity, and horizon glow/reserved. Rebuild only when these inputs change.

The sharp cube is 512². The 128² lighting cube has GGX roughness levels 0..6 and
a cosine-weighted diffuse level 7. Each filtered texel integrates 64 deterministic
directions. This is sky lighting, not local reflection probes or scene GI. The
lighting bake reduces the artistic sky's chroma independently (18% diffuse,
45% specular) and substitutes subdued ground bounce below the horizon. The sharp
background keeps its full blue color. These are explicit art-direction choices,
not a physically calibrated atmosphere. The specular Fresnel approximation has
no BRDF LUT or multiscattering compensation.
Game manual time stays cached; automatic time retains its existing four-per-second
bake cadence. The editor bakes a fixed daytime environment once per device.

`filmic_display` maps peak linear radiance through a smooth toe/shoulder, retaining
RGB ratios so bright blue skies stay blue. Apply exposure and fog in linear light
first, then this transform exactly once, then the consumer's sRGB output encoding.
This is a shared display curve, not a full HDR postprocessing pipeline. Existing
AO remains after the surface display transform in the game. The editor applies
the new curve to pavement and sky; existing building shading is retained.

## Scope

Game pavement and explicit infrastructure concrete receive scans and GGX lighting.
Existing contact deposits, slab joints, snow coverage, and shadow calculations are
retained. Building materials, geometry, facade shaders, and their lighting model
are unchanged; the game-wide display transform and blue sky still affect their
final appearance. Editor building shading is unchanged.

Vehicle meshes use their existing vertex stride: the integer of surface.z remains
the paint variant mask. For untextured GLBs, fractional bits encode a valid bit,
7-bit roughness, 7-bit metallic, and a clearcoat bit. Source UV/albedo textures,
lamp markers, snow exclusions, and alpha-to-coverage remain intact. Textured GLBs
retain a default dielectric finish because their current vertex ABI uses zw for UV;
adding their authored metallic/roughness maps is a separate asset ABI change.

No new shadow pass, bloom, displacement, ray tracing, or building textures are added.
