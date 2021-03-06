﻿using System.Collections;
using System.Collections.Generic;
using UnityEngine;

public class WaterPlaneGen : MonoBehaviour
{
    [SerializeField] private Vector2 size;
    [SerializeField] private int gridSize = 16;

    public Vector2 Size { get => size; set => size = value; }
    public int GridSize { get => gridSize; set => gridSize = value; }

    private MeshFilter filter;

    // Start is called before the first frame update
    private void Awake()
    {
        filter = GetComponent<MeshFilter>();
        filter.mesh = GenerateMesh();
        MapStats.Instance.oceanMesh = filter.mesh;
    }

    private void Start()
    {
        if (MapStats.Instance.oceanMesh != null)
        {
            filter = GetComponent<MeshFilter>();
            filter.mesh = MapStats.Instance.oceanMesh;
        }
    }

    private Mesh GenerateMesh()
    {
        Mesh m = new Mesh();

        List<Vector3> vertices = new List<Vector3>(); // Stores vert x, y, z
        List<Vector3> normals = new List<Vector3>();
        List<Vector2> uvs = new List<Vector2>();      // Stores 2 values (x, z)

        for (int x = 0; x < gridSize + 1; x++)
        {
            for (int y = 0; y < gridSize + 1; y++)
            {
                vertices.Add(new Vector3(-size.x * 0.5f + size.x * (x / (float)gridSize), 0, -size.y * 0.5f + size.y * (y / (float)gridSize)));
                normals.Add(Vector3.up);
                uvs.Add(new Vector2(x / (float)gridSize, y / (float)gridSize));
            }
        }

        List<int> triangles = new List<int>();
        int vertCount = gridSize + 1;
        for (int i = 0; i < vertCount * vertCount - vertCount; i++)
        {
            if ((i + 1) % vertCount == 0)
            {
                continue;
            }
            triangles.AddRange(new List<int>()
            {
                i + 1 + vertCount, i + vertCount, i,
                i, i + 1, i + vertCount + 1
            });
        }

        m.SetVertices(vertices);
        m.SetNormals(normals);
        m.SetUVs(0, uvs);
        m.SetTriangles(triangles, 0);

        return m;
    }
}
