struct PavementMaterial { color:vec3<f32>, normal:vec3<f32>, roughness:f32 };
struct PavementPatch { color:vec3<f32>, bump:vec3<f32>, roughness:f32 };
fn pavement_seed(cell:vec2<f32>)->u32 {
    let p=bitcast<vec2<u32>>(vec2<i32>(cell));
    var h=p.x*374761393u+p.y*668265263u;
    h=(h^(h>>13u))*1274126177u;return h^(h>>16u);
}
fn pavement_turn(p:vec2<f32>,turn:u32)->vec2<f32> {
    switch turn {
        case 1u:{return vec2<f32>(-p.y,p.x);}
        case 2u:{return -p;}
        case 3u:{return vec2<f32>(p.y,-p.x);}
        default:{return p;}
    }
}
fn pavement_patch(uv:vec2<f32>,du:vec2<f32>,dv:vec2<f32>,cell:vec2<f32>,layer:i32)->PavementPatch {
    let seed=pavement_seed(cell+vec2<f32>(f32(layer)*71.0,19.0));
    let turn=(seed>>28u)&3u;
    let scale=0.86+0.28*f32((seed>>16u)&255u)/255.0;
    let offset=vec2<f32>(f32(seed&65535u),f32(pavement_seed(cell+vec2<f32>(37.0,83.0))&65535u))/65536.0;
    let coord=pavement_turn(uv,turn)*scale+offset;
    // Explicit original gradients: random cell boundaries must not choose mips.
    let dx=pavement_turn(du,turn)*scale;let dy=pavement_turn(dv,turn)*scale;
    let color=textureSampleGrad(pavement_color,pavement_sampler,coord,layer,dx,dy).rgb;
    let detail=textureSampleGrad(pavement_detail,pavement_sampler,coord,layer,dx,dy);
    let bump=detail.xyz*2.0-1.0;
    // UV V points down while OpenGL normal Y points up. With this convention
    // the sampled XY normal uses the same quarter turn as the UV transform.
    return PavementPatch(color,vec3<f32>(pavement_turn(bump.xy,turn),bump.z),detail.w);
}
fn pavement_macro_noise(p:vec2<f32>)->f32 {
    let cell=floor(p);let f=fract(p);let t=f*f*(3.0-2.0*f);
    let a=f32(pavement_seed(cell)&65535u)/65535.0;
    let b=f32(pavement_seed(cell+vec2<f32>(1.0,0.0))&65535u)/65535.0;
    let c=f32(pavement_seed(cell+vec2<f32>(0.0,1.0))&65535u)/65535.0;
    let d=f32(pavement_seed(cell+vec2<f32>(1.0,1.0))&65535u)/65535.0;
    return mix(mix(a,b,t.x),mix(c,d,t.x),t.y);
}
// Preserve the road palette's green cycling and red/ochre transit surfaces.
// Neutral paving and the editor's blue-gray theme use the calibrated scan.
fn pavement_lane_color(scan:vec3<f32>,authored:vec3<f32>)->vec3<f32> {
    let cycling=authored.g>authored.r*1.3 && authored.g>authored.b*1.1;
    let transit=authored.r>authored.g*1.35 && authored.r>authored.b*1.35;
    if cycling || transit {return scan*clamp(authored*2.5,vec3<f32>(0.15),vec3<f32>(1.5));}
    return scan;
}
fn pavement_scan_color(scan:vec3<f32>,layer:i32)->vec3<f32> {
    return select(scan*1.10+vec3<f32>(0.012),mix(vec3<f32>(0.27,0.265,0.245),scan*1.32,0.60),layer==1);
}
// A consumer supplies map units in metres and derivatives before divergent masks.
// OpenGL normals: image rows run down, the material bitangent runs up.
fn scanned_pavement(world:vec3<f32>,dx:vec3<f32>,dy:vec3<f32>,n:vec3<f32>,layer:i32)->PavementMaterial {
    var tangent=vec3<f32>(1.0,0.0,0.0);
    if abs(n.z)<0.5 {tangent=vec3<f32>(-n.y,n.x,0.0);}
    else {tangent-=n*dot(tangent,n);}
    tangent*=inverseSqrt(max(dot(tangent,tangent),0.000001));
    let bitangent=cross(n,tangent);
    let meters=select(3.0,2.0,layer==1);
    let uv=vec2<f32>(dot(world,tangent),-dot(world,bitangent))/meters;
    let du=vec2<f32>(dot(dx,tangent),-dot(dx,bitangent))/meters;
    let dv=vec2<f32>(dot(dy,tangent),-dot(dy,bitangent))/meters;
    let footprint=max(length(dx),length(dy));
    let resolved=1.0-smoothstep(0.025,0.20,footprint);
    let average=select(vec3<f32>(0.112067,0.110354,0.098331),vec3<f32>(0.201036,0.169243,0.114603),layer==1);
    let average_roughness=select(0.770377,0.517200,layer==1);
    let roughness_base=select(0.54,0.62,layer==1);
    let macro_lod=max(6.0,log2(max(max(length(du),length(dv))*0.071*1024.0,0.00001)));
    let coarse=textureSampleLevel(pavement_color,pavement_sampler,uv*0.071+vec2<f32>(0.37,0.61),layer,macro_lod).rgb;
    let broad=pavement_macro_noise(uv*0.045+vec2<f32>(13.0,7.0));
    let coarse_variation=1.0+clamp(dot(coarse-average,vec3<f32>(0.2126,0.7152,0.0722))*1.2,-0.10,0.10)+(broad-0.5)*0.10;
    if resolved<=0.0 {
        // Far surfaces retain broad variation with one sample, no fine patches.
        return PavementMaterial(pavement_scan_color(average*coarse_variation,layer),n,clamp(roughness_base+average_roughness*0.30+(1.0-coarse_variation)*0.12,0.58,0.96));
    }
    // Three shared vertices per triangular cell give seamless, world-fixed
    // patch blends. A vertex always has the same transform on either side.
    let grid=vec2<f32>(uv.x,(-uv.x+2.0*uv.y)*0.577350269)*0.65;
    let cell=floor(grid);let f=fract(grid);
    var ids=array<vec2<f32>,3>(cell,cell+vec2<f32>(1.0,0.0),cell+vec2<f32>(0.0,1.0));
    var weights=vec3<f32>(1.0-f.x-f.y,f.x,f.y);
    if f.x+f.y>1.0 {
        ids=array<vec2<f32>,3>(cell+vec2<f32>(1.0),cell+vec2<f32>(0.0,1.0),cell+vec2<f32>(1.0,0.0));
        weights=vec3<f32>(f.x+f.y-1.0,1.0-f.x,1.0-f.y);
    }
    weights=weights*weights*weights;
    weights/=weights.x+weights.y+weights.z;
    let a=pavement_patch(uv,du,dv,ids[0],layer);
    let b=pavement_patch(uv,du,dv,ids[1],layer);
    let c=pavement_patch(uv,du,dv,ids[2],layer);
    let sampled=a.color*weights.x+b.color*weights.y+c.color*weights.z;
    let bump=a.bump*weights.x+b.bump*weights.y+c.bump*weights.z;
    let source_roughness=mix(average_roughness,a.roughness*weights.x+b.roughness*weights.y+c.roughness*weights.z,resolved);
    // Approximate variance compensation prevents three blended scans turning
    // into flat gray at patch junctions; no histogram LUT or new assets needed.
    let contrast=inverseSqrt(dot(weights,weights));
    let fine=max(average+(sampled-average)*contrast,vec3<f32>(0.0));
    let scan=mix(average,fine,resolved)*coarse_variation;
    // Match the approved material study: retain mineral detail without baking
    // the scan's capture contrast into every large concrete surface.
    let albedo=pavement_scan_color(scan,layer);
    let gain=select(0.65,0.40,layer==1)*resolved;
    let perturbed=tangent*bump.x*gain+bitangent*bump.y*gain+n*max(bump.z,0.1);
    let variance=max(0.0,1.0-dot(bump,bump))*resolved;
    let roughness=clamp(roughness_base+source_roughness*0.30+variance*0.12+(1.0-coarse_variation)*0.12,0.58,0.96);
    return PavementMaterial(albedo,perturbed*inverseSqrt(max(dot(perturbed,perturbed),0.000001)),roughness);
}
// GGX direct reflection; callers supply shadow visibility separately.
fn material_direct(base:vec3<f32>,n:vec3<f32>,view:vec3<f32>,sun:vec3<f32>,rough:f32,metal:f32)->vec3<f32> {
    let half_vector=(sun+view)*inverseSqrt(max(dot(sun+view,sun+view),0.000001));
    let nl=max(dot(n,sun),0.0);let nv=max(dot(n,view),0.001);
    let nh=max(dot(n,half_vector),0.0);let vh=max(dot(view,half_vector),0.0);
    let a2=pow(max(rough,0.07),4.0);
    let d=a2/(3.141593*pow(nh*nh*(a2-1.0)+1.0,2.0));
    let k=pow(rough+1.0,2.0)/8.0;
    let visibility=1.0/(4.0*(nv*(1.0-k)+k)*(nl*(1.0-k)+k));
    let f0=mix(vec3<f32>(0.04),base,metal);
    let fresnel=f0+(vec3<f32>(1.0)-f0)*pow(1.0-vh,5.0);
    return ((vec3<f32>(1.0)-fresnel)*base*(1.0-metal)/3.141593+d*visibility*fresnel)*nl;
}
fn material_environment(base:vec3<f32>,n:vec3<f32>,view:vec3<f32>,rough:f32,metal:f32,diffuse:vec3<f32>,reflection:vec3<f32>)->vec3<f32> {
    let nv=max(dot(n,view),0.0);
    let f0=mix(vec3<f32>(0.04),base,metal);
    let fresnel=f0+(max(vec3<f32>(1.0-rough),f0)-f0)*pow(1.0-nv,5.0);
    return base*(1.0-metal)*(vec3<f32>(1.0)-fresnel)*diffuse+reflection*fresnel;
}
