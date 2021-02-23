using System.Collections;
using System.Collections.Generic;
using UnityEngine;

public class WaterNoise : MonoBehaviour
{
    [SerializeField] private float power = 3;
    [SerializeField] private float scale = 1;
    [SerializeField] private float timeScale = 1;

    private float xOffset;
    private float yOffset;
    private MeshFilter mf;

    // Start is called before the first frame update
    void Start()
    {
        mf = GetComponent<MeshFilter>();
        MakeNoise();
    }

    // Update is called once per frame
    void Update()
    {
        MakeNoise();
        xOffset += Time.deltaTime * timeScale;
        if (yOffset <= 0.2f) yOffset += Time.deltaTime * timeScale;
        if (yOffset >= power) yOffset -= Time.deltaTime * timeScale;
    }

    private void MakeNoise()
    {
        Vector3[] vertices = mf.mesh.vertices;

        for (int i = 0; i < vertices.Length; i++)
        {
            vertices[i].y = CalculateHeight(vertices[i].x, vertices[i].z) * power;
        }

        mf.mesh.vertices = vertices;
    }

    private float CalculateHeight(float x, float y)
    {
        float xCoord = x * scale + xOffset;
        float yCoord = y * scale + yOffset;

        return Mathf.PerlinNoise(xCoord, yCoord);
    }
}
