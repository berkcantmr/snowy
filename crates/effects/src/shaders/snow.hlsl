// Snowy effect shader. Follows SHADER_CONTRACT (snowy-core).
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
    float  rot   : TEXCOORD2;
};

VSOut VSMain(VSIn v) {
    float2 corner = float2((v.vid & 1) ? 1.0 : -1.0, (v.vid >> 1) ? 1.0 : -1.0);
    float s, c;
    sincos(v.rot, s, c);
    float2 local = corner * v.size;
    float2 rotated = float2(c * local.x - s * local.y, s * local.x + c * local.y);
    float2 px = v.pos + rotated;
    VSOut o;
    o.sv = float4(px.x / viewport.x * 2.0 - 1.0, 1.0 - px.y / viewport.y * 2.0, 0.0, 1.0);
    o.uv = corner;
    o.color = v.color;
    o.param = v.param;
    o.rot = v.rot;
    return o;
}

// Soft round snowflake. param.x = depth (0 far .. 1 near).
float4 Flake(VSOut i) {
    float d = length(i.uv);
    float body = 1.0 - smoothstep(0.15 + 0.2 * (1.0 - i.param.x), 1.0, d);
    float core = exp(-d * d * 6.0);
    float a = saturate(body * 0.7 + core * 0.5) * i.color.a;
    float3 rgb = lerp(i.color.rgb, float3(1.0, 1.0, 1.0), core * 0.6);
    return float4(rgb * a, a);
}

// Pile blob: a dome sitting on the ledge line. param.x = ledge line in uv.y,
// param.y = half-size in pixels. Overlapping blobs read as one snow cap.
float4 Pile(VSOut i) {
    float bottom = i.param.x;
    float px = i.param.y;
    float yrel = (bottom - i.uv.y) / max(2.0 * bottom, 1e-4); // 0 at ledge, 1 at top
    float f = sqrt(i.uv.x * i.uv.x + yrel * yrel);
    float edge = 1.0 - smoothstep(0.72, 1.0, f);
    float below = saturate((bottom - i.uv.y) * px);
    float a = edge * below * 0.97 * i.color.a;
    float3 rgb = lerp(float3(0.70, 0.79, 0.92), float3(0.96, 0.98, 1.0), smoothstep(0.0, 0.65, yrel));
    float rim = smoothstep(0.6, 0.95, f) * saturate(yrel * 2.5);
    rgb = lerp(rgb, float3(1.0, 1.0, 1.0), rim * 0.8);
    return float4(rgb * a, a);
}

// Shaded snowball. Lighting is screen-aligned; specks rotate with ROT.
float4 Ball(VSOut i) {
    float d = length(i.uv);
    float a = (1.0 - smoothstep(0.9, 1.0, d)) * i.color.a;
    float s, c;
    sincos(i.rot, s, c);
    float2 p = float2(c * i.uv.x - s * i.uv.y, s * i.uv.x + c * i.uv.y); // screen-space
    float z = sqrt(saturate(1.0 - d * d));
    float3 n = float3(p, z);
    float3 L = normalize(float3(-0.5, -0.6, 0.65));
    float diff = saturate(dot(n, L));
    float3 rgb = lerp(float3(0.60, 0.70, 0.88), float3(1.0, 1.0, 1.0), smoothstep(0.05, 0.65, diff));
    float3 H = normalize(L + float3(0, 0, 1));
    rgb += pow(saturate(dot(n, H)), 40.0) * 0.25;
    float sp = 0.0;
    sp += 1.0 - smoothstep(0.06, 0.16, length(i.uv - float2(0.35, 0.10)));
    sp += 1.0 - smoothstep(0.05, 0.13, length(i.uv - float2(-0.30, 0.40)));
    sp += 1.0 - smoothstep(0.05, 0.12, length(i.uv - float2(0.05, -0.45)));
    rgb *= 1.0 - saturate(sp) * 0.10;
    rgb = saturate(rgb);
    return float4(rgb * a, a);
}

// Small irregular soft chunk. param.x = seed.
float4 Chunk(VSOut i) {
    float ang = atan2(i.uv.y, i.uv.x);
    float wob = 1.0 + 0.22 * sin(ang * 3.0 + i.param.x) + 0.12 * sin(ang * 5.0 + i.param.x * 2.3);
    float d = length(i.uv) * wob;
    float a = (1.0 - smoothstep(0.35, 1.0, d)) * i.color.a;
    float3 rgb = lerp(i.color.rgb * float3(0.88, 0.93, 1.0), i.color.rgb, saturate(0.5 - i.uv.y * 0.5));
    return float4(rgb * a, a);
}

float4 PSMain(VSOut i) : SV_Target {
    int kind = (int)(i.param.w + 0.5);
    if (kind == 1) return Pile(i);
    if (kind == 2) return Ball(i);
    if (kind == 3) return Chunk(i);
    return Flake(i);
}
