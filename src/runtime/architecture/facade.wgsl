// Shared editor/game facade evaluation. UVs count bays/floors; all derivative
// footprints are prepared by the caller before material-dependent branches.
fn architecture_hash(p:vec2<f32>)->f32 {
    let q=bitcast<vec2<u32>>(vec2<i32>(p));
    var h=q.x*374761393u+q.y*668265263u;h=(h^(h>>13u))*1274126177u;
    return f32(h^(h>>16u))/4294967295.0;
}
fn architecture_glass_tint(tint_id:u32)->vec3<f32> {
    // Subtle material absorption; the reflected environment supplies the sky hue.
    switch tint_id {
        case 1u: {return vec3<f32>(0.60,0.74,0.70);}
        case 2u: {return vec3<f32>(0.78,0.80,0.80);}
        case 3u: {return vec3<f32>(0.34,0.37,0.38);}
        default: {return vec3<f32>(0.62,0.72,0.79);}
    }
}
fn architecture_sky(ray:vec3<f32>,day:f32)->vec3<f32> {
    let h=clamp(ray.z*0.75+0.35,0.0,1.0);
    var sky=mix(mix(vec3<f32>(0.03,0.06,0.12),vec3<f32>(0.68,0.78,0.83),day),
        mix(vec3<f32>(0.01,0.025,0.08),vec3<f32>(0.20,0.40,0.67),day),pow(h,0.6));
    let azimuth=atan2(ray.x,ray.y)*10.0;
    let top=0.02+architecture_hash(vec2<f32>(floor(azimuth),3.0))*0.25;
    let skyline=1.0-smoothstep(top-0.015,top+0.015,ray.z);
    return mix(sky,mix(vec3<f32>(0.025,0.045,0.075),vec3<f32>(0.2,0.32,0.42),day),skyline*0.72);
}
fn architecture_room(uv:vec2<f32>,aspect:f32,seed:f32,n:vec3<f32>,eye:vec3<f32>,occupied:f32,daylight:f32)->vec3<f32> {
    let variation=architecture_hash(vec2<f32>(seed,19.0));
    let depth=1.8*mix(0.75,1.25,variation);
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
    var albedo=mix(vec3<f32>(0.54,0.43,0.30),vec3<f32>(0.35,0.47,0.45),architecture_hash(vec2<f32>(seed,3.0)));
    if travel.x<=travel.y && travel.x<=travel.z {
        inward=vec3<f32>(select(1.0,-1.0,ray.x>0.0),0.0,0.0);
        edges=vec2<f32>(min(hit.y,1.0-hit.y),min(hit.z,depth-hit.z));
    } else if travel.y<=travel.z {
        inward=vec3<f32>(0.0,select(1.0,-1.0,ray.y>0.0),0.0);
        edges=vec2<f32>(min(hit.x,aspect-hit.x),min(hit.z,depth-hit.z));
        albedo=select(vec3<f32>(0.26,0.18,0.12),vec3<f32>(0.76,0.73,0.65),ray.y>0.0);
    }
    let ao=(0.4+0.6*smoothstep(0.0,0.16,edges.x))*(0.4+0.6*smoothstep(0.0,0.16,edges.y));
    let lamp=vec3<f32>(aspect*mix(0.3,0.7,variation),0.88,depth*0.45)-hit;
    let lamp_distance=max(length(lamp),0.001);
    let lambert=max(dot(inward,lamp)/lamp_distance,0.0);
    let warmth=mix(vec3<f32>(1.0,0.65,0.33),vec3<f32>(0.78,0.87,1.0),architecture_hash(vec2<f32>(seed,7.0))*0.6);
    let power=occupied*mix(1.8,3.2,architecture_hash(vec2<f32>(seed,11.0)))*mix(1.0,0.10,daylight);
    let window_fill=daylight*(0.10+0.45/(1.0+hit.z*hit.z*2.0));
    var color=albedo*ao*(vec3<f32>(0.035,0.045,0.07)+vec3<f32>(0.65,0.78,1.0)*window_fill+warmth*power*(0.16+lambert/(0.35+lamp_distance*lamp_distance)));
    if inward.z< -0.5 {
        let q=hit.xy/size.xy;
        let picture=step(0.2,q.x)*step(q.x,0.65)*step(0.43,q.y)*step(q.y,0.75);
        color=mix(color,albedo*0.22+vec3<f32>(0.10,0.12,0.09)*power,picture*0.8);
        color*=1.0-step(q.y,0.26)*step(0.12,q.x)*step(q.x,0.85)*0.5;
    }
    if inward.y< -0.5 {
        let q=vec2<f32>(hit.x/aspect,hit.z/depth);
        let fixture=step(0.25,q.x)*step(q.x,0.75)*step(0.38,q.y)*step(q.y,0.48);
        color+=fixture*warmth*power*0.65;
    }
    return color;
}
fn architecture_facade(uv:vec2<f32>,footprint:vec2<f32>,packed:f32,
    n:vec3<f32>,eye:vec3<f32>,base:vec3<f32>,environment:vec3<f32>,
    lighting:vec3<f32>,day:f32,plain:bool,glazing:bool)->vec3<f32> {
    let flags=u32(round(packed));let family=flags&7u;
    let palette=(flags>>3u)&15u;let tint_id=(flags>>7u)&3u;
    let age=f32((flags>>9u)&15u)/15.0;let seed=f32((flags>>13u)&127u);
    let street=((flags>>20u)&1u)==1u;let pattern=(flags>>21u)&3u;
    let aa=max(footprint,vec2<f32>(0.0005));let cell=floor(uv);let q=fract(uv);
    if plain&&family==6u {
        let grain=architecture_hash(floor(uv*vec2<f32>(296.4,308.75))+vec2<f32>(seed,0.0));
        return base*lighting*(0.85+grain*0.28);
    }
    if glazing {
        let view=eye/max(length(eye),0.00001);
        let grazing=pow(1.0-clamp(dot(n,view),0.0,1.0),5.0);
        let tint=architecture_glass_tint(tint_id);
        return mix(base*lighting*0.20,environment*tint,0.45+grazing*0.4);
    }
    if family==6u {
        let r=length(q-vec2<f32>(0.5));
        let rings=step(0.16,fract(r*30.0));
        let blades=step(0.5,sin(atan2(q.y-0.5,q.x-0.5)*7.0+r*9.0));
        let grille=select(mix(0.25,0.7,step(0.25,fract(q.y*8.0))),
            mix(0.18,0.8,rings)*mix(0.5,1.0,blades),abs(n.z)>0.5);
        return base*lighting*grille;
    }
    var x0=0.18;var x1=0.82;var y0=0.16;var y1=0.88;
    let old=family==3u||family==4u||family==7u;
    if family==0u||family==2u {x0=0.015;x1=0.985;y0=0.15;y1=0.985;}
    if family==3u||family==7u {x0=0.25;x1=0.75;y0=0.16;y1=0.84;}
    if family==4u {x0=0.23;x1=0.77;y0=0.13;y1=0.88;}
    if family==5u {x0=0.12;x1=0.88;y0=0.23;y1=0.82;}
    if family==1u {
        if pattern==0u {x0=select(0.12,0.30,cell.x%3.0==0.0);x1=0.91;}
        else if pattern==1u {x0=0.07;x1=0.93;y0=select(0.13,0.35,cell.y%3.0==0.0);}
        else {x0=0.19;x1=0.81;y0=0.20;}
    }
    if street {x0=select(0.05,0.18,old);x1=1.0-x0;y0=0.08;y1=0.90;}
    var top=y1;
    let arch=family==4u||family==7u||(family==3u&&street);
    if arch {let a=(q.x-0.5)/((x1-x0)*0.5);top=y1-0.22+0.22*sqrt(max(0.0,1.0-a*a));}
    let edges=min(q-vec2<f32>(x0,y0),vec2<f32>(x1,top)-q);
    let mask=smoothstep(-aa.x*0.4,aa.x*0.6,edges.x)*smoothstep(-aa.y*0.4,aa.y*0.6,edges.y);
    var course=vec2<f32>(2.8,7.2);
    if palette==7u {course=vec2<f32>(1.1,2.2);}
    if palette==8u {course=vec2<f32>(6.5,14.0);}
    let row=floor(uv.y*course.y);
    let stone_uv=uv*course+vec2<f32>((row%2.0)*0.5,0.0);
    let stone_f=fract(stone_uv);let stone_aa=max(aa*course,vec2<f32>(0.001));
    let joint=max(1.0-smoothstep(0.007,0.007+stone_aa.x,min(stone_f.x,1.0-stone_f.x)),
        1.0-smoothstep(0.01,0.01+stone_aa.y,min(stone_f.y,1.0-stone_f.y)));
    let variation=mix(0.91,1.07,architecture_hash(floor(stone_uv)));
    let streak=pow(architecture_hash(vec2<f32>(floor(uv.x*3.0),cell.y)),3.0);
    var wall=base*variation*(1.0-joint*0.23)*(1.0-age*(0.12+streak*0.35))*lighting;
    if plain {return wall;}
    if family==0u {wall=vec3<f32>(0.055,0.14,0.24)*lighting;}
    if old {
        let sill=(1.0-smoothstep(0.025,0.025+aa.y,abs(q.y-y0+0.035)))
            *step(x0-0.06,q.x)*step(q.x,x1+0.06);
        wall*=1.0+sill*0.25;
    }
    let room_seed=seed+cell.x*31.0+cell.y*19.0;
    let room_uv=clamp((q-vec2<f32>(x0,y0))/vec2<f32>(x1-x0,y1-y0),vec2<f32>(0.0),vec2<f32>(1.0));
    let occupied=step(architecture_hash(vec2<f32>(room_seed,23.0)),0.58);
    var interior=vec3<f32>(0.04,0.08,0.12);
    if max(aa.x,aa.y)<0.8 {
        interior=architecture_room(room_uv,0.96*(x1-x0)/(y1-y0),room_seed,n,eye,occupied,day);
    }
    let tint=architecture_glass_tint(tint_id);
    let view=eye/max(length(eye),0.00001);
    let grazing=pow(1.0-clamp(dot(n,view),0.0,1.0),5.0);
    let reflection=clamp(mix(0.09,0.48,day)*select(0.45,1.0,family==0u||family==2u)+grazing*0.42,0.0,0.94);
    var glass=mix(interior*vec3<f32>(0.76,0.78,0.76),environment*tint,reflection);
    var mullion=1.0-smoothstep(0.006,0.006+aa.x,abs(q.x-0.5));
    if old {mullion=max(mullion,1.0-smoothstep(0.009,0.009+aa.y,abs(q.y-(y0+y1)*0.5)));}
    glass=mix(glass,select(vec3<f32>(0.12,0.19,0.24),base*0.22,old),mullion*0.75);
    return mix(wall,glass,mask);
}
