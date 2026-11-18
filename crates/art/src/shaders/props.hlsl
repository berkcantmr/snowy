// Snowy props shader: draws every snowy_art::PrimKind inside a rotated, padded square quad.
// Follows SHADER_CONTRACT (snowy-core). Distances are in pixels (the quad is never scaled),
// so edge anti-aliasing is a constant one-pixel ramp.
// PARAM = (box half-extent x / SIZE, box half-extent y / SIZE, round, kind).
cbuffer Frame : register(b0) { float2 viewport; float time; float _pad; };

struct VSIn {
    float2 pos   : POS;
    float  size  : SIZE;
    float  rot   : ROT;
    float4 color : COLOR;
    float4 param : PARAM;
    uint   vid   : SV_VertexID;
};

struct VSOut {
    float4 sv    : SV_Position;
    float2 uv    : TEXCOORD0;
    float4 color : COLOR;
    float4 param : TEXCOORD1;
    float  size  : TEXCOORD2;
};

VSOut VSMain(VSIn i) {
    float2 c = float2((i.vid & 1) ? 1.0 : -1.0, (i.vid & 2) ? 1.0 : -1.0);
    float s, co;
    sincos(i.rot, s, co);
    float2 p = float2(c.x * co - c.y * s, c.x * s + c.y * co) * i.size + i.pos;
    VSOut o;
    o.sv = float4(p.x / viewport.x * 2.0 - 1.0, 1.0 - p.y / viewport.y * 2.0, 0.0, 1.0);
    o.uv = c;
    o.color = i.color;
    o.param = i.param;
    o.size = i.size;
    return o;
}

static const float PI = 3.14159265;

float sdEllipse(float2 p, float2 h) {
    return (length(p / h) - 1.0) * min(h.x, h.y);
}

float sdRoundRect(float2 p, float2 h, float rad) {
    float2 q = abs(p) - h + rad;
    return length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - rad;
}

// Apex at the top (y down: -h.y), base at the bottom; base corners rounded by `rad`.
float sdTriangle(float2 p, float2 h, float rad) {
    float2 q = float2(abs(p.x), p.y + h.y);
    float2 nrm = normalize(float2(2.0 * h.y, -h.x));
    float d1 = dot(q, nrm);
    float d2 = p.y - h.y;
    float2 v = float2(d1, d2) + rad;
    return length(max(v, 0.0)) + min(max(v.x, v.y), 0.0) - rad;
}

float sdCapsule(float2 p, float2 h) {
    float a = max(h.x - h.y, 0.0);
    return length(p - float2(clamp(p.x, -a, a), 0.0)) - h.y;
}

// Five-pointed star in the unit circle (tip up), scaled by the box.
float sdStar(float2 p, float2 h) {
    float2 n = p / h;
    float ang = atan2(n.x, -n.y);
    float sector = 2.0 * PI / 5.0;
    ang = abs(fmod(ang + sector * 0.5 + 10.0 * sector, sector) - sector * 0.5);
    float len = length(n);
    float2 pt = len * float2(cos(ang), sin(ang));
    float2 A = float2(1.0, 0.0);
    float2 B = 0.42 * float2(cos(PI / 5.0), sin(PI / 5.0));
    float2 e = B - A;
    float2 w = pt - A;
    float dd = length(w - e * saturate(dot(w, e) / dot(e, e)));
    float side = e.x * w.y - e.y * w.x;
    return (side > 0.0 ? -dd : dd) * min(h.x, h.y);
}

float4 PSMain(VSOut i) : SV_Target {
    float2 h = max(i.param.xy * i.size, 0.01);
    float2 p = i.uv * i.size;
    float mh = min(h.x, h.y);
    float rnd = i.param.z;
    int kind = (int)(i.param.w + 0.5);
    float3 rgb = i.color.rgb;
    float alpha = i.color.a;
    float sd = 1.0e3;

    if (kind == 0) {
        sd = sdEllipse(p, h);
    } else if (kind == 1) {
        sd = sdEllipse(p, h);
        float2 n = p / h;
        float d = length(n);
        float nz = sqrt(saturate(1.0 - d * d));
        float diff = saturate(dot(float3(n, nz), normalize(float3(-0.5, -0.6, 0.62))));
        float3 cool = float3(0.70, 0.78, 0.97);
        rgb = rgb * lerp(cool, float3(1.0, 1.0, 1.0), smoothstep(0.0, 0.7, diff));
        float rim = smoothstep(0.78, 1.0, d) * (0.45 + 0.55 * saturate(dot(n / max(d, 0.001), float2(0.6, 0.8))));
        rgb = lerp(rgb, rgb * float3(0.6, 0.68, 0.9), rim * 0.7);
        float hl = smoothstep(0.3, 0.0, length(n - float2(-0.38, -0.42)));
        rgb = lerp(rgb, float3(1.0, 1.0, 1.0), hl * 0.45);
    } else if (kind == 2) {
        sd = sdRoundRect(p, h, rnd * mh);
    } else if (kind == 3) {
        sd = sdTriangle(p, h, rnd * mh * 0.5);
    } else if (kind == 4) {
        sd = sdCapsule(p, h);
    } else if (kind == 5) {
        sd = sdStar(p, h);
    } else if (kind == 6) {
        float r = length(p / h);
        float halo = pow(saturate(1.0 - r), 2.2);
        float core = 1.0 - smoothstep(0.16, 0.36, r);
        rgb = lerp(rgb, float3(1.0, 1.0, 1.0), core * 0.6);
        float a = saturate(halo * 0.75 + core) * alpha;
        return float4(rgb * a, a);
    } else if (kind == 8) {
        // Ellipse clipped to a band (fabric on a ball). round packs lo/hi (10 bits each).
        float v = round(rnd * 1048576.0);
        float hq = floor(v / 1024.0);
        float lo = (v - hq * 1024.0) / 1023.0 * 2.0 - 1.0;
        float hi = hq / 1023.0 * 2.0 - 1.0;
        float e = sdEllipse(p, h);
        // +1 = top of the ellipse; screen y grows downward.
        float band = max(-hi * h.y - p.y, p.y + lo * h.y);
        sd = max(e, band);
        float2 n = p / h;
        float d = length(n);
        float f = -n.y;
        rgb *= lerp(0.84, 1.0, saturate(f * 0.5 + 0.5));
        rgb *= 1.0 - smoothstep(0.72, 1.0, d) * (0.18 + 0.14 * saturate(n.x));
    } else {
        float e = sdEllipse(p, h);
        sd = max(e, -e - rnd * mh);
    }

    float a = saturate(0.5 - sd) * alpha;
    return float4(rgb * a, a);
}
