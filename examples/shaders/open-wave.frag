// Ripple bloom: a twisting wave assembles the window, refracts its content,
// and leaves a brief cyan/violet crest. The small block fringe echoes close-pixel.
// Progress must use linear timing so the waves have time to travel and settle.

float wave_hash(vec2 p) {
    return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453123);
}

vec4 wave_sample(vec2 uv) {
    if (uv.x < 0.0 || uv.y < 0.0 || uv.x >= 1.0 || uv.y >= 1.0)
        return vec4(0.0);
    return texture2D(tex, uv * halley_tex_scale + halley_tex_offset);
}

vec4 open_color(vec3 coords_geo, vec3 size_geo) {
    float progress = clamp(halley_clamped_progress, 0.0, 1.0);
    if (progress <= 0.0)
        return vec4(0.0);
    if (progress >= 0.995)
        return wave_sample(coords_geo.xy);

    vec2 size_px = max(size_geo.xy, vec2(1.0));
    float short_side = max(min(size_px.x, size_px.y), 1.0);
    vec2 aspect = size_px / short_side;
    float seed = halley_random_seed;
    vec2 origin = vec2(0.5) + vec2(sin(seed * 6.283185), cos(seed * 6.283185)) * 0.055;
    vec2 point = (coords_geo.xy - origin) * aspect;
    float radius = length(point);
    vec2 radial = point / max(radius, 0.001);
    vec2 tangent = vec2(-radial.y, radial.x);
    float extent = length(max(origin, vec2(1.0) - origin) * aspect);

    // Broad spiral unfurls, with two finer ripples travelling behind its crest.
    // All deformation decays to zero before the live window takes over.
    float unsettled = 1.0 - smoothstep(0.26, 0.96, progress);
    float twist = 0.90 * unsettled * exp(-radius * 1.25);
    float c = cos(twist);
    float s = sin(twist);
    vec2 source_point = vec2(c * point.x - s * point.y, s * point.x + c * point.y);
    float ripple = sin(radius * 34.0 - progress * 19.0 + seed * 6.283185);
    float crosswave = sin(point.y * 22.0 + point.x * 9.0 + progress * 13.0);
    source_point += radial * ripple * 0.042 * unsettled;
    source_point += tangent * crosswave * 0.028 * unsettled;
    vec2 source_uv = origin + source_point / aspect;

    // A gently broken wavefront, using the same block scale as the close shader.
    float block_px = clamp(short_side / 52.0, 8.0, 16.0);
    vec2 grid = max(floor(size_px / block_px), vec2(1.0));
    vec2 cell = floor(coords_geo.xy * grid);
    float random_cell = wave_hash(cell + vec2(seed * 97.0, seed * 35.89));
    float angle = atan(point.y, point.x);
    float scallop = sin(angle * 5.0 + progress * 8.0 + seed * 6.283185) * 0.025 * unsettled;
    float front = extent * (progress * 1.40 - 0.06);
    float distance_to_front = radius + scallop + (random_cell - 0.5) * 0.045 * unsettled - front;
    float reveal = 1.0 - smoothstep(-0.060, 0.035, distance_to_front);
    reveal *= smoothstep(0.0, 0.07, progress);

    // Refraction and a narrow chromatic fringe, strongest at the moving crest.
    float crest = exp(-abs(distance_to_front) * 32.0) * unsettled;
    vec2 split = radial * (0.007 * unsettled + 0.009 * crest) / aspect;
    vec4 color = wave_sample(source_uv);
    vec4 red_sample = wave_sample(source_uv + split);
    vec4 blue_sample = wave_sample(source_uv - split);
    color.rgb = mix(color.rgb, vec3(red_sample.r, color.g, blue_sample.b), 0.65 * unsettled);

    // Only the arriving fringe briefly resolves into solid pixel chunks.
    vec2 chunk_uv = (floor(source_uv * grid) + vec2(0.5)) / grid;
    vec4 chunk = wave_sample(chunk_uv);
    color = mix(color, chunk, crest * 0.38);
    vec3 crest_color = mix(vec3(0.22, 0.85, 1.0), vec3(0.72, 0.38, 1.0),
        0.5 + 0.5 * sin(angle * 3.0 + progress * 9.0));
    color.rgb += crest_color * color.a * crest * 0.30;
    color.rgb = clamp(color.rgb, vec3(0.0), vec3(color.a));
    return color * reveal;
}
