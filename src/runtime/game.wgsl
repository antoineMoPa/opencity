// Optional standalone shader. Z up, map units. Vertex layout is runtime::Vertex.
// Group 0 binding 0: column-major view_projection, light_view_projection, eye.xyz/time, sun.xyz/exposure.
// Group 1: 2048-square depth shadow texture (0), comparison sampler (1).
struct Camera { view_projection: mat4x4<f32>, light_view_projection: mat4x4<f32>, eye_time: vec4<f32>, sun_exposure: vec4<f32> };
@group(0) @binding(0) var<uniform> camera: Camera;
@group(1) @binding(0) var shadow_map: texture_depth_2d;
@group(1) @binding(1) var shadow_sampler: sampler_comparison;
struct Input { @location(0) position: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) color: vec3<f32>, @location(3) surface: vec4<f32> };
struct Output { @builtin(position) clip: vec4<f32>, @location(0) world: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) color: vec3<f32>, @location(3) @interpolate(flat) surface: vec4<f32> };
@vertex fn vs(v: Input) -> Output { var o: Output; o.clip=camera.view_projection*vec4<f32>(v.position,1.0);o.world=v.position;o.normal=v.normal;o.color=v.color;o.surface=v.surface;return o; }
fn hash(p:vec2<f32>)->f32 {let q=bitcast<vec2<u32>>(vec2<i32>(p));var h=q.x*374761393u+q.y*668265263u;h=(h^(h>>13u))*1274126177u;return f32(h^(h>>16u))/4294967295.0;}
fn noise(p:vec2<f32>)->f32 {let i=floor(p);let f=fract(p);let u=f*f*(3.0-2.0*f);return mix(mix(hash(i),hash(i+vec2<f32>(1.0,0.0)),u.x),mix(hash(i+vec2<f32>(0.0,1.0)),hash(i+vec2<f32>(1.0,1.0)),u.x),u.y);}
@vertex fn vs_shadow(v:Input)->@builtin(position) vec4<f32> {return camera.light_view_projection*vec4<f32>(v.position,1.0);}
fn visibility(world:vec3<f32>,n:vec3<f32>)->f32 {
    let clip=camera.light_view_projection*vec4<f32>(world+n*0.00025,1.0);
    let p=clip.xyz/clip.w;let uv=vec2<f32>(p.x*0.5+0.5,0.5-p.y*0.5);
    var value=0.0;
    for (var x=-1;x<=1;x++) {for (var y=-1;y<=1;y++) {value+=textureSampleCompareLevel(shadow_map,shadow_sampler,uv+vec2<f32>(f32(x),f32(y))/2048.0,p.z-0.0001);}}
    return select(1.0,value/9.0,all(uv>vec2<f32>(0.0))&&all(uv<vec2<f32>(1.0))&&p.z>0.0&&p.z<1.0);
}
@fragment fn fs(v:Output)->@location(0) vec4<f32> {
    var base=v.color;let p=v.world.xy;let kind=v.surface.x;
    var n=v.normal / max(length(v.normal),0.00001);
    let detail=1.0-smoothstep(0.4,1.4,max(length(dpdx(p*1600.0)),length(dpdy(p*1600.0))));
    let grain=mix(0.5,noise(p*1600.0),detail);let broad=noise(p*28.0)*0.65+noise(p*83.0)*0.35;
    let grid=abs(fract(p/0.045)-0.5);let joint=1.0-smoothstep(0.46,0.49,max(grid.x,grid.y));
    let floor_position=(v.world.z-v.surface.y)/max(v.surface.z,0.001);
    let floor_aa=max(fwidth(floor_position),0.001);
    let band=1.0-smoothstep(0.025,0.025+floor_aa,min(fract(floor_position),1.0-fract(floor_position)));
    if kind<0.5 { // Asphalt aggregate, weathering, occasional repaired cracks.
        let crack=pow(1.0-abs(sin(p.x*307.0+noise(p*16.0)*14.0)),32.0)*0.16;
        base*=0.35*(0.80+0.20*grain+0.15*broad-crack);
    } else if kind<1.5 { // Multi-scale grass cover, soil and blades.
        base=mix(base*0.62,base*1.35,broad)*(0.88+0.24*grain);
    } else if kind<2.5 {base*=mix(0.65,1.0,joint)*(0.90+grain*0.16);}
    else if kind<3.5 { // Editor facade palette/floor spacing with game lighting.
        let across=select(v.world.y,v.world.x,abs(n.y)>abs(n.x));
        let window=step(0.22,fract(across/0.025))*step(0.35,fract(floor_position))*step(fract(floor_position),0.85);
        let facing=1.0-abs(n.z);
        if v.surface.w<1.5 {base=mix(base,vec3<f32>(0.045,0.12,0.17),window*facing*0.65);}
        else if v.surface.w>2.5 {let mortar=step(0.92,fract(floor_position*8.0))+step(0.94,fract(across*230.0+floor(floor_position*8.0)*0.5));base=mix(base,vec3<f32>(0.2),min(mortar,1.0)*0.2);}
        if v.surface.w>1.5 {base=mix(base,vec3<f32>(0.055,0.11,0.14),window*facing*0.65);}
        base=mix(base,vec3<f32>(0.55,0.58,0.60),band*facing*0.7);
    } else if kind<4.5 {
        let wave=sin(p.x*120.0+camera.eye_time.w*1.2)+sin(p.y*190.0-camera.eye_time.w*0.7);
        n=vec3<f32>(0.08*cos(p.x*120.0+camera.eye_time.w*1.2),0.08*cos(p.y*190.0-camera.eye_time.w*0.7),1.0);
        n/=length(n);base*=0.92+wave*0.04;
    } else if kind<5.5 {base*=0.86+0.25*grain;}
    let sun=camera.sun_exposure.xyz / length(camera.sun_exposure.xyz);
    let diffuse=max(dot(n,sun),0.0)*visibility(v.world,n);let hemi=0.35+0.15*max(n.z,0.0);
    let eye=camera.eye_time.xyz-v.world;let view=eye/max(length(eye),0.001);
    let halfway=(sun+view)/max(length(sun+view),0.001);
    var spec=pow(max(dot(n,halfway),0.0),48.0)*0.06;
    if kind>3.5&&kind<4.5 {spec=pow(max(dot(n,halfway),0.0),120.0)*1.3;}
    var lit=base*(hemi+diffuse*0.95)+vec3<f32>(1.0,0.92,0.78)*spec;
    let distance=length(eye);let fog=1.0-exp(-distance*0.025)*(1.0-smoothstep(5.8,7.6,distance));
    // Exposure and filmic compression. Surface target is sRGB.
    lit*=camera.sun_exposure.w;lit=lit/(lit+vec3<f32>(1.0));
    return vec4<f32>(mix(lit,vec3<f32>(0.24,0.50,0.82),fog),1.0);
}
