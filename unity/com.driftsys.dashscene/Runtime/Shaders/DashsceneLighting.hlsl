// The light term the two lit material classes take.
//
// **A dashscene document carries no lighting intent**, and nothing on boundary
// B says which node is lit — so this is not resolved per node. The material
// class is a host setting, and a host that chooses a lit class is asking for
// the whole document to take the engine's light. Inventing a per-node lighting
// flag would be discovering vocabulary rather than validating it, which P4
// forbids, and P1 keeps results out of the document in the first place.
//
// The surface is the quad, so its normal is the quad's: object-space -Z, which
// faces the camera in the painter's own orientation. There is no normal on
// boundary B either, and a painter that made one up per node would be inventing
// the same vocabulary from the other end.

#ifndef DASHSCENE_LIGHTING_INCLUDED
#define DASHSCENE_LIGHTING_INCLUDED

#include "Packages/com.unity.render-pipelines.universal/ShaderLibrary/Lighting.hlsl"

// Lambert against URP's main light, plus the ambient probe.
//
// Deliberately not a full BRDF. A UI quad has no material parameters on
// boundary B — no metallic, no smoothness, no normal map — so every term
// beyond diffuse would be a constant this file chose, and a constant chosen
// here is a picture no document can predict.
float3 DsLit(float3 albedo)
{
    float3 normalWS = normalize(TransformObjectToWorldDir(float3(0.0, 0.0, -1.0)));
    Light mainLight = GetMainLight();
    float ndotl = saturate(dot(normalWS, mainLight.direction));
    float3 direct = mainLight.color * mainLight.distanceAttenuation * ndotl;
    float3 ambient = SampleSH(normalWS);
    return albedo * (direct + ambient);
}

#endif // DASHSCENE_LIGHTING_INCLUDED
