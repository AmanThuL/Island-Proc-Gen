using UnityEngine;

[System.Serializable]
public class NoiseSettings
{
    public int seed;
    public bool randomizedSeed;

    public Vector2 offset;

    //public int octaves = 6;
    //[Range(0, 1)] public float persistence = 0.6f;
    //public float lacunarity = 2;
    [Range(2, 10)] public int exponent = 4;
    [Range(0f, 1f)] public float oceanRatio = 0.5f;

    [Range(0f, 3f)] public float scale = 1.1f;
    [Range(1f, 3f)] public float moistureDistributionRatio = 1.5f;
}
