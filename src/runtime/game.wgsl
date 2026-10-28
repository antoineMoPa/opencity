// Optional standalone shader. Z up, map units. Vertex layout is runtime::Vertex.
// Group 0 binding 0: column-major view_projection, light_view_projection, eye.xyz/time, sun.xyz/exposure, daylight/sky.rgb.
// Group 1: 2048-square depth shadow texture (0), comparison sampler (1).
struct Camera { view_projection: mat4x4<f32>, light_view_projection: mat4x4<f32>, eye_time: vec4<f32>, sun_exposure: vec4<f32>, daylight_sky: vec4<f32> };
@group(0) @binding(0) var<uniform> camera: Camera;
@group(1) @binding(0) var shadow_map: texture_depth_2d;
@group(1) @binding(1) var shadow_sampler: sampler_comparison;
struct Input { @location(0) position: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) color: vec3<f32>, @location(3) surface: vec4<f32> };
struct Output { @builtin(position) clip: vec4<f32>, @location(0) world: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) color: vec3<f32>, @location(3) @interpolate(flat) surface: vec4<f32>, @location(4) fence_uv: vec2<f32> };
@vertex fn vs(v: Input) -> Output { var o: Output; o.clip=camera.view_projection*vec4<f32>(v.position,1.0);o.world=v.position;o.normal=v.normal;o.color=v.color;o.surface=v.surface;o.fence_uv=v.surface.yz;return o; }
fn hash(p:vec2<f32>)->f32 {let q=bitcast<vec2<u32>>(vec2<i32>(p));var h=q.x*374761393u+q.y*668265263u;h=(h^(h>>13u))*1274126177u;return f32(h^(h>>16u))/4294967295.0;}
fn noise(p:vec2<f32>)->f32 {let i=floor(p);let f=fract(p);let u=f*f*(3.0-2.0*f);return mix(mix(hash(i),hash(i+vec2<f32>(1.0,0.0)),u.x),mix(hash(i+vec2<f32>(0.0,1.0)),hash(i+vec2<f32>(1.0,1.0)),u.x),u.y);}
// Local panel coordinates keep line patterns attached when a prop rotates as debris.
fn fence_coverage(uv:vec2<f32>,style:f32,footprint:vec2<f32>)->f32 {
    let p=vec2<f32>(uv.x,uv.y*0.85);
    let aa=max(length(footprint*vec2<f32>(1.0,0.85))*0.5,0.0007);
    let post=abs(fract(p.x/1.2+0.5)-0.5)*1.2-0.018;
    let rails=min(abs(p.y-0.10),abs(p.y-0.75))-0.014;
    var signed_distance:f32;
    if style<0.5 {
        let cell=(fract(p.x/0.12+0.5)-0.5)*0.12;
        let rod=max(abs(cell)-0.006,p.y-0.78);
        let spear=max(abs(cell)-max(0.0,(0.85-p.y)*0.20),max(0.76-p.y,p.y-0.85));
        let ring=abs(length(vec2<f32>((fract(p.x/0.24)-0.5)*0.24,p.y-0.58))-0.038)-0.004;
        signed_distance=min(min(post,rails),min(rod,min(spear,ring)));
    } else {
        let a=abs(fract((p.x+p.y)/0.12+0.5)-0.5)*0.12;
        let b=abs(fract((p.x-p.y)/0.12+0.5)-0.5)*0.12;
        signed_distance=min(min(post,rails),min(a,b)-0.0025);
    }
    return 1.0-smoothstep(-aa,aa,signed_distance);
}
struct ShadowOutput { @builtin(position) clip:vec4<f32>, @location(0) fence_uv:vec2<f32>, @location(1) @interpolate(flat) kind_pattern:vec2<f32> };
@vertex fn vs_shadow(v:Input)->ShadowOutput {
    var o:ShadowOutput;o.clip=camera.light_view_projection*vec4<f32>(v.position,1.0);
    o.fence_uv=v.surface.yz;o.kind_pattern=v.surface.xw;return o;
}
@fragment fn fs_shadow(v:ShadowOutput) {
    let footprint=fwidth(v.fence_uv);
    if v.kind_pattern.x>10.5&&v.kind_pattern.x<11.5 {
        if fence_coverage(v.fence_uv,v.kind_pattern.y,footprint)<0.5 {discard;}
    }
}
fn visibility(world:vec3<f32>,n:vec3<f32>)->f32 {
    let clip=camera.light_view_projection*vec4<f32>(world+n*0.00025,1.0);
    let p=clip.xyz/clip.w;let uv=vec2<f32>(p.x*0.5+0.5,0.5-p.y*0.5);
    var value=0.0;
    for (var x=-1;x<=1;x++) {for (var y=-1;y<=1;y++) {value+=textureSampleCompareLevel(shadow_map,shadow_sampler,uv+vec2<f32>(f32(x),f32(y))/2048.0,p.z-0.0001);}}
    return select(1.0,value/9.0,all(uv>vec2<f32>(0.0))&&all(uv<vec2<f32>(1.0))&&p.z>0.0&&p.z<1.0);
}
// Ray exits an open-front box: two side walls, floor, ceiling and back wall.
// Dimensions are relative to the window height; the ray needs no unit length.
fn room_interior(uv:vec2<f32>,aspect:f32,seed:f32,n:vec3<f32>,eye:vec3<f32>,occupied:f32)->vec3<f32> {
    let variation=hash(vec2<f32>(seed,19.0));
    let depth=mix(0.85,2.3,variation);
    let size=vec3<f32>(aspect,1.0,depth);
    let origin=vec3<f32>(clamp(uv,vec2<f32>(0.0001),vec2<f32>(0.9999))*size.xy,0.0);
    let tangent=vec3<f32>(n.y,-n.x,0.0);
    let ray=vec3<f32>(-dot(eye,tangent),-eye.z,max(dot(eye,n),0.00001));
    let exit_plane=select(vec3<f32>(0.0),size,ray>=vec3<f32>(0.0));
    // Parallel rays must never select a side face, including at the pane edge.
    let travel=select(vec3<f32>(1e10), (exit_plane-origin)/select(vec3<f32>(1.0),ray,abs(ray)>vec3<f32>(0.000001)),abs(ray)>vec3<f32>(0.000001));
    let t=min(travel.x,min(travel.y,travel.z));
    let hit=clamp(origin+ray*t,vec3<f32>(0.0),size);
    var inward=vec3<f32>(0.0,0.0,-1.0);
    var edges=min(hit.xy,size.xy-hit.xy);
    var albedo=mix(vec3<f32>(0.52,0.43,0.32),vec3<f32>(0.36,0.49,0.47),hash(vec2<f32>(seed,3.0)));
    if travel.x<=travel.y && travel.x<=travel.z {
        inward=vec3<f32>(select(1.0,-1.0,ray.x>0.0),0.0,0.0);
        edges=vec2<f32>(min(hit.y,1.0-hit.y),min(hit.z,depth-hit.z));
    } else if travel.y<=travel.z {
        inward=vec3<f32>(0.0,select(1.0,-1.0,ray.y>0.0),0.0);
        edges=vec2<f32>(min(hit.x,aspect-hit.x),min(hit.z,depth-hit.z));
        albedo=select(vec3<f32>(0.22,0.14,0.085),vec3<f32>(0.76,0.72,0.63),ray.y>0.0);
    }
    let ao=(0.42+0.58*smoothstep(0.0,0.18,edges.x))*(0.42+0.58*smoothstep(0.0,0.18,edges.y));
    let lamp=vec3<f32>(aspect*mix(0.3,0.7,variation),0.88,depth*0.45)-hit;
    let lamp_distance=max(length(lamp),0.001);
    let lambert=max(dot(inward,lamp)/lamp_distance,0.0);
    let daylight=clamp(camera.daylight_sky.x,0.0,1.0);
    let warmth=mix(vec3<f32>(1.0,0.62,0.30),vec3<f32>(0.78,0.86,1.0),hash(vec2<f32>(seed,7.0))*0.65);
    let power=occupied*mix(1.5,3.2,hash(vec2<f32>(seed,11.0)))*mix(1.0,0.18,daylight);
    let window_fill=daylight*(0.12+0.65/(1.0+hit.z*hit.z*2.0));
    return albedo*ao*(vec3<f32>(0.018,0.025,0.045)+vec3<f32>(0.65,0.78,1.0)*window_fill+warmth*power*(0.16+lambert/(0.35+lamp_distance*lamp_distance)));
}
@fragment fn fs(v:Output,@builtin(front_facing) front:bool)->@location(0) vec4<f32> {
    var base=v.color;let p=v.world.xy;let kind=v.surface.x;var facade_glow=0.0;
    var facade_room=vec3<f32>(0.0);var room_mask=0.0;
    var n=v.normal / max(length(v.normal),0.00001);
    let fence_footprint=fwidth(v.fence_uv);
    var coverage=1.0;
    if kind>10.5&&kind<11.5 {
        coverage=fence_coverage(v.fence_uv,v.surface.w,fence_footprint);
        if !front {n=-n;}
    }
    let detail=1.0-smoothstep(0.4,1.4,max(length(dpdx(p*1600.0)),length(dpdy(p*1600.0))));
    let grain=mix(0.5,noise(p*1600.0),detail);let broad=noise(p*28.0)*0.65+noise(p*83.0)*0.35;
    let grid=abs(fract(p/0.045)-0.5);let joint=1.0-smoothstep(0.46,0.49,max(grid.x,grid.y));
    let floor_position=(v.world.z-v.surface.y)/max(v.surface.z,0.001);
    let floor_aa=max(fwidth(floor_position),0.001);
    let band=1.0-smoothstep(0.025,0.025+floor_aa,min(fract(floor_position),1.0-fract(floor_position)));
    // Derivatives must be evaluated before material-dependent control flow on WebGPU.
    let brick_across=select(v.world.y,v.world.x,abs(n.y)>abs(n.x));
    let brick_row=floor_position*45.0;
    let brick_coord=brick_across/max(v.surface.z*0.07,0.00001)+floor(brick_row)*0.5;
    let brick_detail=1.0-smoothstep(0.25,0.8,max(fwidth(brick_row),fwidth(brick_coord)));
    let roof_unit=max(v.surface.y,0.000001);
    let roof_row=(p.x+p.y)/(roof_unit*0.15);
    let roof_tile=(p.y-p.x)/(roof_unit*0.32)+floor(roof_row)*0.5;
    let roof_detail=1.0-smoothstep(0.25,0.8,max(fwidth(roof_row),fwidth(roof_tile)));
    if coverage<0.01 {discard;}
    if kind<0.5 { // Asphalt aggregate, weathering, occasional repaired cracks.
        let crack=pow(1.0-abs(sin(p.x*307.0+noise(p*16.0)*14.0)),32.0)*0.16;
        base*=0.35*(0.80+0.20*grain+0.15*broad-crack);
    } else if kind<1.5 { // Multi-scale grass cover, soil and blades.
        base=mix(base,vec3<f32>(0.18,0.22,0.15),0.45);
        base=mix(base*0.72,base*1.22,broad)*(0.90+0.18*grain);
    } else if kind<2.5 {base*=mix(0.65,1.0,joint)*(0.90+grain*0.16);}
    else if kind<3.5 { // Editor facade palette/floor spacing with game lighting.
        let across=select(v.world.y,v.world.x,abs(n.y)>abs(n.x));
        let window=step(0.22,fract(across/0.025))*step(0.35,fract(floor_position))*step(fract(floor_position),0.85);
        let facing=1.0-abs(n.z);
        let lit_cell=step(0.45,hash(vec2<f32>(floor(across/0.025),floor(floor_position))));
        facade_glow=window*facing*lit_cell;
        if v.surface.w>10.5 {
            // Detailed facades supply their own window geometry. Limestone uses
            // larger courses; the other palettes use staggered brick texture.
            let is_stone=step(14.5,v.surface.w);
            let row=mix(brick_row,brick_row*0.45,is_stone);
            let column=mix(brick_coord,brick_coord*0.45,is_stone);
            let mortar=max(step(0.93,fract(row)),step(0.96,fract(column)));
            let variation=0.92+hash(vec2<f32>(floor(column),floor(row)))*0.16;
            base=mix(base,base*variation*(1.0-mortar*0.28),brick_detail*facing);
            facade_glow=0.0;
        } else if v.surface.w<1.5 {base=mix(base,vec3<f32>(0.045,0.12,0.17),window*facing*0.65);}
        else if v.surface.w>2.5 {
            // Mortar stays in the shader; no brick or stone is geometry.
            let mortar=max(step(0.93,fract(brick_row)),step(0.96,fract(brick_coord)));
            let variation=0.9+hash(vec2<f32>(floor(brick_coord),floor(brick_row)))*0.2;
            base=mix(base,base*variation*(1.0-mortar*0.35),brick_detail);
        }
        if v.surface.w>1.5 && v.surface.w<10.5 {base=mix(base,vec3<f32>(0.055,0.11,0.14),window*facing*0.65);}
        if v.surface.w<10.5 {base=mix(base,vec3<f32>(0.55,0.58,0.60),band*facing*0.7);}
        if v.surface.w<10.5 && facing>0.9 {
            let fraction=fract(across/0.025);
            let axis_sign=select(-n.x,n.y,abs(n.y)>abs(n.x));
            let room_u=select((1.0-fraction)/0.78,(fraction-0.22)/0.78,axis_sign>0.0);
            let uv=vec2<f32>(room_u,(fract(floor_position)-0.35)/0.5);
            let aspect=0.0195/max(v.surface.z*0.5,0.001);
            let seed=hash(vec2<f32>(floor(across/0.025),floor(floor_position)))*255.0;
            facade_room=room_interior(uv,aspect,seed,n,camera.eye_time.xyz-v.world,lit_cell);
            room_mask=window;
            facade_glow=0.0;
        }
    } else if kind<4.5 {
        let wave=sin(p.x*120.0+camera.eye_time.w*1.2)+sin(p.y*190.0-camera.eye_time.w*0.7);
        n=vec3<f32>(0.08*cos(p.x*120.0+camera.eye_time.w*1.2),0.08*cos(p.y*190.0-camera.eye_time.w*0.7),1.0);
        n/=length(n);base*=0.92+wave*0.04;
    } else if kind<5.5 {base*=0.86+0.25*grain;}
    else if kind>9.5&&kind<10.5 {
        // A small, anti-aliased shingle pattern scaled by the consumer's unit.
        let gap=max(step(0.95,fract(roof_row)),step(0.97,fract(roof_tile)));
        base*=1.0-roof_detail*gap*0.3;
    }
    let sun=camera.sun_exposure.xyz / length(camera.sun_exposure.xyz);
    let daylight=clamp(camera.daylight_sky.x,0.0,1.0);
    let diffuse=max(dot(n,sun),0.0)*visibility(v.world,n);let hemi=mix(0.12,0.35,daylight)+mix(0.05,0.15,daylight)*max(n.z,0.0);
    let eye=camera.eye_time.xyz-v.world;let view=eye/max(length(eye),0.001);
    let halfway=(sun+view)/max(length(sun+view),0.001);
    var spec=pow(max(dot(n,halfway),0.0),48.0)*0.06;
    if kind>3.5&&kind<4.5 {spec=pow(max(dot(n,halfway),0.0),120.0)*1.3;}
    let ambient_tint=mix(vec3<f32>(0.42,0.56,0.85),vec3<f32>(1.0),daylight);
    let sunlight=mix(vec3<f32>(0.40,0.52,0.80),vec3<f32>(1.0,0.92,0.78),daylight);
    var lit=base*(ambient_tint*hemi+sunlight*diffuse*mix(0.12,0.95,daylight))+sunlight*spec*mix(0.15,1.0,daylight);
    let darkness=pow(1.0-daylight,2.0);
    lit+=vec3<f32>(0.9,0.48,0.18)*facade_glow*darkness;
    lit=mix(lit,facade_room,room_mask);
    if kind>7.5&&kind<8.5 {
        let lit_window=step(0.05,base.r);
        if v.surface.w>=16.0 {
            let seed=floor(v.surface.w/16.0);
            let aspect=max(v.surface.w-seed*16.0,0.125);
            let interior=room_interior(v.fence_uv,aspect,seed,n,eye,lit_window);
            let fresnel=0.04+0.65*pow(1.0-max(dot(n,view),0.0),5.0);
            lit=mix(interior,camera.daylight_sky.yzw*0.38,fresnel);
        } else {
            lit+=vec3<f32>(1.4,0.72,0.27)*darkness*lit_window;
        }
    } else if kind>8.5&&kind<9.5 {
        lit+=vec3<f32>(1.3,0.85,0.4)*darkness;
    }
    let distance=length(eye);let fog=1.0-exp(-distance*0.025)*(1.0-smoothstep(5.8,7.6,distance));
    // Exposure and filmic compression. Surface target is sRGB.
    lit*=camera.sun_exposure.w;lit=lit/(lit+vec3<f32>(1.0));
    return vec4<f32>(mix(lit,camera.daylight_sky.yzw,fog),coverage);
}
