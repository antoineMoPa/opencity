// Hue-preserving filmic display transform. Apply once, after exposure and fog.
// Operate on peak radiance so a blue sky cannot become white merely because
// independent channel curves converge on one. Output remains linear for sRGB.
fn filmic_display(radiance:vec3<f32>)->vec3<f32> {
    let color=max(radiance,vec3<f32>(0.0));
    let peak=max(color.r,max(color.g,color.b));
    let shoulder=clamp((peak*(2.51*peak+0.03))/(peak*(2.43*peak+0.59)+0.14),0.0,1.0);
    return color*(shoulder/max(peak,0.000001));
}
