// Material-space infrastructure weathering. Derivatives are supplied uniformly by the caller.
fn pavement_edges(across:f32,packed:f32)->vec2<f32> {
    let bits=u32(max(round(packed),0.0));
    let width=f32((bits>>2u)&4095u)*0.0001;
    if width<=0.0 {return vec2<f32>(0.0);}
    let codes=vec2<u32>((bits>>14u)&31u,(bits>>19u)&31u);
    let foot=select(vec2<f32>(0.0),vec2<f32>(0.002),codes==vec2<u32>(31u));
    let distance=max(vec2<f32>(across,width-across)-foot,vec2<f32>(0.0));
    let height=max(vec2<f32>(codes)-vec2<f32>(1.0),vec2<f32>(0.0))*0.0001;
    let contact=select(vec2<f32>(0.0),vec2<f32>(1.0)-distance/
        sqrt(distance*distance+height*height+vec2<f32>(1e-12)),codes>vec2<u32>(1u));
    let dirt=select(vec2<f32>(0.0),exp(-distance/0.004),codes>vec2<u32>(0u));
    return vec2<f32>(max(contact.x,contact.y),max(dirt.x,dirt.y));
}
fn pavement_weather(p:vec2<f32>,footprint:vec2<f32>,base:vec3<f32>,asphalt:bool,
    edge:vec2<f32>,joint:f32)->vec3<f32> {
    let meters=p*100.0;let aa=footprint*100.0;
    let mottling=architecture_noise(meters*0.22,aa*0.22,61.0);
    let deposits=architecture_noise(meters*1.3,aa*1.3,83.0);
    let aggregate=architecture_noise(meters*24.0,aa*24.0,107.0);
    let stain=smoothstep(0.48,0.86,mottling)*smoothstep(0.25,0.85,deposits);
    let dirt=select(0.16,0.21,asphalt)*stain+edge.y*0.17+joint*0.12;
    var color=base*(0.91+aggregate*0.16)*(1.0-dirt);
    color=mix(color,color*vec3<f32>(0.86,0.82,0.74),dirt*0.7);
    // Curb occlusion is local to real raised neighbors; flush lane seams remain clear.
    return color*(1.0-edge.x*0.24);
}
fn concrete_weather(world:vec3<f32>,footprint:vec3<f32>,height:f32,
    n:vec3<f32>,base:vec3<f32>)->vec3<f32> {
    let vertical=1.0-abs(n.z);
    let meters=select(world.xy,vec2<f32>(select(world.y,world.x,abs(n.y)>abs(n.x)),world.z),vertical>0.5)*100.0;
    let aa=select(footprint.xy,vec2<f32>(max(footprint.x,footprint.y),footprint.z),vertical>0.5)*100.0;
    let damp=exp(-max(height,0.0)/0.009)*vertical;
    let runoff=pow(architecture_noise(meters*vec2<f32>(2.2,0.12),aa*vec2<f32>(2.2,0.12),139.0),2.0);
    let patches=architecture_noise(meters*0.45,aa*0.45,151.0);
    let pores=architecture_noise(meters*31.0,aa*31.0,173.0);
    let dirt=0.15*patches+0.20*runoff*vertical+0.19*damp;
    let contact=0.20*damp+select(0.0,0.12,n.z< -0.5);
    return base*(0.92+pores*0.15)*(1.0-dirt)*(1.0-contact)
        *mix(vec3<f32>(1.0),vec3<f32>(0.84,0.82,0.75),dirt);
}
