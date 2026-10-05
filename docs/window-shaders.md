# Custom window open and close shaders

This is an advanced feature. Ordinary open and close styles stay in
`docs/animations.md`. A custom shader is optional and off unless you set a
fragment-shader path.

`type` still chooses the geometry timeline. The shader, when it compiles,
replaces scale and fade. `launch` and `retract` still travel; in-place types
(`center-out`, `fade`, `shrink`) keep the real window rectangle. Node collapse
never uses a shader.

```rune
animations:
  window-open:
    type "launch"
    duration-ms 220
    curve "ease-out-cubic"
    custom-shader "shaders/open.frag"
  end
  window-close:
    type "retract"
    duration-ms 270
    custom-shader "shaders/close.frag"
  end
end
```

Relative paths resolve from the directory that contains `halley.rune`. `~/`
expands through the current home directory. Halley recompiles when the path or
file mtime changes. A read or compile failure is logged once and the
configured `type` draws instead.

The repository includes a [wave opening shader](../examples/shaders/open-wave.frag)
with spiral ripples, refraction, a cyan/violet crest, and a pixel fringe. Copy
it to `~/.config/halley/open-wave.frag` and use these settings in your existing
`animations.window-open` block:

```rune
window-open:
  enabled true
  type "center-out"
  duration-ms 900
  curve "linear"
  custom-shader "open-wave.frag"
end
```

The example settles to the current client texture before the animation ends.
Client content can continue loading and animating while the wave runs.

The file is not a full program. Halley wraps it with Smithay's texture-shader
header (`//_DEFINES_`, `v_coords`, `tex`, `alpha`) and an epilogue `main`.
Your source must define one function:

- open: `vec4 open_color(vec3 coords_geo, vec3 size_geo)`
- close: `vec4 close_color(vec3 coords_geo, vec3 size_geo)`

For opening shaders, the snapshot is the complete current window scene:
client pixels, decorations, opacity, backdrop blur, and shadow. It is rendered
at the presentation size so the settled shader matches normal live rendering.
The client continues receiving frame callbacks while the shader runs, so newly
committed content is included in subsequent shader frames. Its geometry
includes the visible shadow extent. The backdrop is captured from
the real output before applying the shader; transparency is preserved outside
the scene. Closing shaders continue to use the decorated closing snapshot.

`coords_geo.xy` is 0 to 1 inside the snapshot geometry and may be
outside that range because the shader runs on a padded quad. `size_geo.xy` is
that geometry in compositor pixels. Return premultiplied alpha.

Uniforms you may use:

- `tex` — the window snapshot
- `halley_progress` — motion value. Open may overshoot with springs or
  elastic. Close is linear wall-clock time; the CPU ease-in-out used by
  shrink/fade is not applied to the shader.
- `halley_clamped_progress` — that value clamped to 0..1
- `halley_random_seed` — stable in `[0, 1)` for the life of the animation
- `halley_tex_scale` and `halley_tex_offset` — map geometry to `tex`
- `halley_geo_size` — same as `size_geo.xy`
- `alpha` — final snapshot opacity. Opening snapshots already contain the
  separate client and decoration opacities, so this is 1 for opening. For
  closing it carries client opacity (window-rule and cluster fade). Do not
  apply it yourself; the epilogue multiplies it.

Sample the snapshot like this:

```glsl
vec2 coords_tex = coords_geo.xy * halley_tex_scale + halley_tex_offset;
vec4 color = texture2D(tex, coords_tex);
```

Custom window shaders own the complete animated silhouette. During opening,
the shadow is part of the shader's input and follows its deformation instead
of being drawn as a separate rectangle. Closing shaders omit the ordinary
geometry-based shadow, which cannot follow fragmented or dissolved pixels.

The shader interface is not a compatibility guarantee. Opening windows that
are also fullscreen, maximized, or arranging keep the live tree. Closing
windows that collapse into a node keep the CPU collapse.

In-flight open and close animations keep the shader path chosen when they
started. A newly compiled program is used on the next frame.
