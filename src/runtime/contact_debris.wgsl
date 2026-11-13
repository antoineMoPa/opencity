// Hosts bind a linear RGBA8 mip chain, repeat-U/clamp-V sampler and settings uniform.
// Coordinates and footprints are in map units (100 meters per unit).
fn contact_deposits(base:vec3<f32>,uv:vec2<f32>,aa:vec2<f32>,vertical:bool)->vec3<f32> {
    // The full spread keeps wall grime visible above the raised sidewalk surface.
    let spread=contact_settings.y;
    if contact_settings.w<0.5 || uv.y<0.0 || uv.y>=spread {return base;}
    let scale=vec2<f32>(0.08,0.0065);
    // Explicit LOD is valid inside material/coverage branches and after masked discards.
    let texel_footprint=max(aa/scale*vec2<f32>(1024.0,128.0),vec2<f32>(1.0));
    let lod=log2(max(texel_footprint.x,texel_footprint.y));
    let masks=textureSampleLevel(contact_atlas,contact_sampler,uv/scale,lod);
    let band=pow(max(0.0,1.0-uv.y/spread),1.5);
    let grime=band*(0.3+0.7*masks.r)*contact_settings.x;
    var color=mix(base,base*vec3<f32>(0.27,0.235,0.19),grime);
    if !vertical {
        let footprint=max(aa.x,aa.y)*100.0;
        let coverage=1.0-smoothstep(spread*0.7,spread,uv.y);
        let stones=(1.0-smoothstep(0.004,0.018,footprint))*contact_settings.z*coverage;
        let leaves=(1.0-smoothstep(0.015,0.05,footprint))*contact_settings.z*coverage;
        // Explicit linear-light counterparts of the prototype's display colors.
        color=mix(color,vec3<f32>(0.0685,0.0660,0.0509),masks.g*stones);
        color=mix(color,vec3<f32>(0.0470,0.0220,0.0054),masks.b*leaves);
        color=mix(color,vec3<f32>(0.393,0.330,0.187),masks.a*leaves*0.85);
    }
    return color;
}
fn pavement_deposits(base:vec3<f32>,uv:vec2<f32>,aa:vec2<f32>,packed:f32)->vec3<f32> {
    if packed< -4.0 {
        // CPU clipping already restricts the finite edge; U retains the source arc phase.
        return contact_deposits(base,uv,aa,false);
    }
    let bits=u32(max(round(packed),0.0));
    let width=f32((bits>>2u)&4095u)*0.0001;
    if width<=0.0 {return base;}
    let left=(bits>>14u)&31u;let right=(bits>>19u)&31u;
    // Automatic guard-wall coordinates mark its centerline, while the visible
    // face projects 20 cm into the pavement. Authored raised neighbors use their face.
    let a=select(1e6,max(uv.y-select(0.0,0.002,left==31u),0.0),left>1u);
    let b=select(1e6,max(width-uv.y-select(0.0,0.002,right==31u),0.0),right>1u);
    // Select one contact; overlapping narrow bands cannot multiply darkness twice.
    return contact_deposits(base,vec2<f32>(uv.x,min(a,b)),aa,false);
}
