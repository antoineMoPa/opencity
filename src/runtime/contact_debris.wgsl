// Hosts bind a linear RGBA8 mip chain, repeat-U/clamp-V sampler and settings uniform.
// Coordinates and footprints are in map units (100 meters per unit).
fn contact_deposits(base:vec3<f32>,uv:vec2<f32>,aa:vec2<f32>,vertical:bool)->vec3<f32> {
    return seasonal_contact_deposits(base,uv,aa,vertical,0.0);
}
fn seasonal_contact_deposits(base:vec3<f32>,uv:vec2<f32>,aa:vec2<f32>,vertical:bool,winter:f32)->vec3<f32> {
    // The full spread keeps wall grime visible above the raised sidewalk surface.
    let spread=contact_settings.y;
    if contact_settings.w<0.5 || uv.y<0.0 || uv.y>=spread {return base;}
    let scale=vec2<f32>(0.08,0.0065);
    // Explicit LOD is valid inside material/coverage branches and after masked discards.
    let texel_footprint=max(aa/scale*vec2<f32>(1024.0,128.0),vec2<f32>(1.0));
    let lod=log2(max(texel_footprint.x,texel_footprint.y));
    let masks=textureSampleLevel(contact_atlas,contact_sampler,uv/scale,lod);
    let band=pow(max(0.0,1.0-uv.y/spread),1.5);
    let runoff=architecture_noise(vec2<f32>(uv.x*170.0,uv.y*640.0),aa*vec2<f32>(170.0,640.0),23.0);
    let grime=band*(0.3+0.7*masks.r)*contact_settings.x*mix(0.65,1.35,runoff);
    if winter>0.0 {
        // Reuse the contact atlas for irregular snow caught along curbs and crevices.
        // This narrow deposit survives road clearing, including on vertical curb bases.
        let snow=band*(0.75+0.25*masks.r)*clamp(winter,0.0,1.0);
        return mix(base,vec3<f32>(0.86,0.92,0.96),snow);
    }
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
    return seasonal_pavement_deposits(base,uv,aa,packed,0.0);
}
fn seasonal_pavement_deposits(base:vec3<f32>,uv:vec2<f32>,aa:vec2<f32>,packed:f32,winter:f32)->vec3<f32> {
    if packed< -4.0 {
        // CPU clipping already restricts the finite edge; U retains the source arc phase.
        return seasonal_contact_deposits(base,uv,aa,false,winter);
    }
    let bits=u32(max(round(packed),0.0));
    let width=f32((bits>>2u)&4095u)*0.0001;
    if width<=0.0 {return base;}
    let left=(bits>>14u)&31u;let right=(bits>>19u)&31u;
    // Automatic guard-wall coordinates mark its centerline, while the visible
    // face projects 20 cm into the pavement. Authored raised neighbors use their face.
    let a=select(1e6,max(uv.y-select(0.0,0.002,left==31u),0.0),left>3u);
    let b=select(1e6,max(width-uv.y-select(0.0,0.002,right==31u),0.0),right>3u);
    // Select one contact; overlapping narrow bands cannot multiply darkness twice.
    return seasonal_contact_deposits(base,vec2<f32>(uv.x,min(a,b)),aa,false,winter);
}

// Band codes 1/2/3 describe an exposed rim, a turf contact, and a curb lip.
// Sample both sides of turf contacts in world space so soil pockets line up.
fn broken_surface_edge(base:vec3<f32>, world:vec2<f32>, aa:vec2<f32>,
    distance:f32, code:u32, turf:bool)->vec3<f32> {
    if code==0u || code>3u || distance>0.0065 || (turf && code!=2u) {return base;}
    let breakup=architecture_noise(world*260.0,aa*260.0,47.0);
    let chips=architecture_noise(world*1900.0,aa*1900.0,79.0);
    let grit=architecture_noise(world*5200.0,aa*5200.0,97.0);
    let footprint=max(aa.x,aa.y);
    let width=select(0.0012,select(0.003,0.006,turf),code==2u);
    let reach=width*(0.06+breakup*breakup+0.22*chips);
    let filter_width=max(select(0.00009,0.00048,code==2u),footprint*0.65);
    let mask=(1.0-smoothstep(reach-filter_width,reach+filter_width,max(distance,0.0)))
        *(1.0-smoothstep(width,width*4.0,footprint));
    if code==3u {
        // Chips reveal pale cement aggregate in isolated pockets along the raised lip.
        return mix(base,base*(0.66+grit*0.90),mask*smoothstep(0.34,0.74,breakup));
    }
    let loam=vec3<f32>(0.080,0.061,0.039)*(0.56+0.65*chips+0.30*breakup);
    let gravel=mix(loam,vec3<f32>(0.24,0.22,0.17),smoothstep(0.55,0.86,grit));
    return mix(base,gravel,mask*select(0.54,0.79,code==2u));
}
fn road_band_edge(base:vec3<f32>,world:vec2<f32>,aa:vec2<f32>,uv:vec2<f32>,packed:f32,turf:bool)->vec3<f32> {
    if packed<4.0 {return base;}
    let bits=u32(round(packed));
    let width=f32((bits>>2u)&4095u)*0.0001;
    let left=(bits>>14u)&31u;
    let right=(bits>>19u)&31u;
    let across=uv.y;
    let use_left=left>0u && (right==0u || across<width-across);
    let distance=select(width-across,across,use_left);
    let code=select(right,left,use_left);
    var color=base;
    if code==16u && distance<0.0048 {
        // Fifteen-centimetre sidewalk rise. Offset the drains on opposite gutters.
        let along=abs(fract(uv.x/0.27+select(0.22,0.68,use_left))-0.5)*0.27;
        if along<0.0046 {
            let pixel=max(0.00004,max(aa.x,aa.y));
            let inset=min(0.0041-along,0.0021-abs(distance-0.0025));
            let coverage=smoothstep(-pixel,pixel,inset)*(1.0-smoothstep(0.0006,0.002,pixel));
            let bar=abs(fract(uv.x/0.00087)-0.5)*0.00087;
            let slot=smoothstep(0.00013-pixel,0.00013+pixel,bar)*smoothstep(0.00022,0.00040,inset);
            color=mix(color,mix(vec3<f32>(0.061,0.057,0.050),vec3<f32>(0.012),slot),coverage);
        }
    }
    if code==3u && distance<0.0021 {
        // A narrow curb course has its own back seam and offset block joints.
        let pixel=max(0.00003,max(aa.x,aa.y));
        let course=1.0-smoothstep(0.0016-pixel,0.0016+pixel,distance);
        let back=abs(distance-0.0016);
        let joint=abs(fract(uv.x/0.012+0.29)-0.5)*0.012;
        let groove=1.0-smoothstep(0.000025,0.000075+pixel,min(back,joint));
        let resolved=1.0-smoothstep(0.0002,0.0015,pixel);
        color*=1.0+resolved*(0.07*course-0.29*groove);
    }
    return broken_surface_edge(color,world,aa,distance,code,turf);
}
