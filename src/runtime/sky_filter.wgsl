@group(0) @binding(0) var source:texture_cube<f32>;
@group(0) @binding(1) var source_sampler:sampler;
struct Ray { @builtin(position) clip:vec4<f32>, @location(0) ray:vec3<f32>, @location(1) @interpolate(flat) level:u32 };
fn lighting_radiance(ray:vec3<f32>,level:u32)->vec3<f32> {
    let radiance=textureSampleLevel(source,source_sampler,ray,0.0).rgb;
    let luminance=dot(radiance,vec3<f32>(0.2126,0.7152,0.0722));
    // The artistic display sky is deliberately saturated. Lighting has its own
    // chroma, and the lower hemisphere represents subdued ground bounce.
    let chroma=select(0.45,0.18,level==7u);
    let sky=mix(vec3<f32>(luminance),radiance,chroma);
    let ground=vec3<f32>(0.24,0.23,0.21)*luminance;
    return mix(ground,sky,smoothstep(-0.20,0.0,ray.z));
}
@vertex fn vs(@builtin(vertex_index) vertex:u32,@builtin(instance_index) index:u32)->Ray {
    let p=vec2(f32((vertex<<1u)&2u),f32(vertex&2u))*2.0-1.0;
    let uv=vec2(p.x,-p.y);var ray:vec3<f32>;
    switch index%6u {
        case 0u:{ray=vec3(1.0,-uv.y,-uv.x);} case 1u:{ray=vec3(-1.0,-uv.y,uv.x);}
        case 2u:{ray=vec3(uv.x,1.0,uv.y);} case 3u:{ray=vec3(uv.x,-1.0,-uv.y);}
        case 4u:{ray=vec3(uv.x,-uv.y,1.0);} default:{ray=vec3(-uv.x,-uv.y,-1.0);}
    }
    return Ray(vec4(p,0.0,1.0),ray,index/6u);
}
@fragment fn fs(v:Ray)->@location(0) vec4<f32> {
    let n=v.ray*inverseSqrt(dot(v.ray,v.ray));
    if v.level==0u {return vec4(lighting_radiance(n,v.level),1.0);}
    let axis=select(vec3(0.0,0.0,1.0),vec3(1.0,0.0,0.0),abs(n.z)>0.99);
    let cross_axis=cross(axis,n);let t=cross_axis*inverseSqrt(dot(cross_axis,cross_axis));let b=cross(n,t);
    let rough=f32(v.level)/6.0;let a2=pow(rough,4.0);
    var sum=vec3(0.0);var weight=0.0;
    for(var i=0u;i<64u;i++) {
        let u=(f32(i)+0.5)/64.0;
        let phi=6.2831853*f32(reverseBits(i))*2.3283064365386963e-10;
        var cosine=sqrt((1.0-u)/(1.0+(a2-1.0)*u));
        if v.level==7u {cosine=sqrt(1.0-u);}
        let sine=sqrt(max(0.0,1.0-cosine*cosine));
        let h=t*cos(phi)*sine+b*sin(phi)*sine+n*cosine;
        var ray=2.0*dot(n,h)*h-n;
        if v.level==7u {ray=h;}
        let w=select(max(dot(n,ray),0.0),1.0,v.level==7u);
        sum+=lighting_radiance(ray,v.level)*w;weight+=w;
    }
    return vec4(sum/max(weight,0.0001),1.0);
}
