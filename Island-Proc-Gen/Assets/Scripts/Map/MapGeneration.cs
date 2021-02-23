using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using Assets.Map;
using RTS_Cam;
using UnityEngine.SceneManagement;

public enum Biome
{
    OCEAN,
    BEACH,
    GRASSLAND,
    LAKE,
    FOREST,
    RAIN_FOREST,
    TEMPERATE_DESERT,
    DESERT
}

// 0 1 2
// 3 4 5
// 6 7 8
public enum Region
{
    Northwest,
    North,
    Northeast,
    West,
    Middle,
    East,
    Southwest,
    South,
    Southeast
}

public enum ItemEquipment
{
    Watermelon,
    Starbucks,
    Sofa,
    Axe,
    Hammer
}

[System.Serializable] public class BiomeTileMeshDict : SerializableDictionary<Biome, GameObject> { }

public class MapGeneration : MonoBehaviour
{
    [Header("Prefabs")]
    [SerializeField]
    private GameObject treePrefab;
    public GameObject terrainTilePrefab;
    [SerializeField]
    private List<GameObject> rockVariationPrefabs;
    [SerializeField]
    private GameObject theaterPrefab;
    [SerializeField]
    private Transform playerTransform;

    public GameObject watermelonPrefab, starbucksPrefab, sofaPrefab;
    public GameObject axePrefab, hammerPrefab;
    public GameObject enemyPrefab;

    [Header("General Properties")]
    [SerializeField]
    private float tileLength;
    // Set map width and height HERE!
    [SerializeField]
    private int width;
    [SerializeField]
    private int height;
    [SerializeField]
    private bool generateTrees;
    [SerializeField]
    private bool generateRocks;
    private GameObject oceanObj;
    private GameObject tilesParent, treesParent, rocksParent, buildingsParent, itemsParent, enemiesParent;
    private Graph graph;

    [Header("Biome/Mesh")]
    [SerializeField]
    private BiomeGameObjectDictionary biomeDict;

    [Header("RTS Camera Settings")]
    [SerializeField]
    [Range(0f, 1f)] private float camLimitXScale;
    [SerializeField]
    [Range(0f, 1f)] private float camLimitYScale;

    public NoiseSettings noiseSettings = new NoiseSettings();

    [Header("Poisson Disc Sampling Attributes")]
    [SerializeField]
    [Range(1f, 2f)] private float poissonRadius = 1f;


    // Start is called before the first frame update
    void Start()
    {
        // Store width/height in static class for further use
        MapStats.Instance.mapWidth = width;
        MapStats.Instance.mapHeight = height;
        MapStats.Instance.tileLength = tileLength;

        oceanObj = GameObject.Find("Ocean");
        tilesParent = GameObject.Find("Tiles");
        treesParent = GameObject.Find("Trees");
        rocksParent = GameObject.Find("Rocks");
        buildingsParent = GameObject.Find("Buildings");
        itemsParent = GameObject.Find("Items/Equipments");
        enemiesParent = GameObject.Find("Enemies");

        if (SceneManager.GetActiveScene().name == "MainIsland")
            noiseSettings.seed = MapStats.Instance.seed;
        graph = new Graph(height, width, noiseSettings, poissonRadius);

        int longerSide = (int)(Mathf.Max(width, height) * 1.7f);

        //oceanObj.GetComponent<WaterPlaneGen>().Size = new Vector2(tileLength * longerSide * 1.5f, tileLength * longerSide * 1.5f);
        //oceanObj.GetComponent<WaterPlaneGen>().GridSize = longerSide;
        //oceanObj.GetComponent<BoxCollider>().size = new Vector3(tileLength * longerSide * 1.5f, 1, tileLength * longerSide * 1.5f);

        // Set camera limit
        //RTS_Camera mainCam = GameObject.Find("Main Camera").GetComponent<RTS_Camera>();
        //mainCam.limitX = width * tileLength * camLimitXScale;
        //mainCam.limitY = height * tileLength * camLimitYScale;

        // =====================================

        MapStats.Instance.tileData = graph.CreateTerrainTilesData(tileLength);

        // Create tiles in scene
        for (int i = 0; i < width; i++)
        {
            for (int j = 0; j < height; j++)
            {
                Tile tempTile = MapStats.Instance.tileData[i, j];
                GameObject tileObj = Instantiate(terrainTilePrefab,
                                                 new Vector3(tempTile.Center.x, 0, tempTile.Center.y),
                                                 Quaternion.Euler(new Vector3(0, Random.Range(0, 4) * 90, 0)));

                // Set localscale of tile gameobject
                tileObj.transform.localScale = new Vector3(tileLength, tileObj.transform.localScale.y, tileLength);

                // Create a tile object based on biome type
                GameObject biomeTile = Instantiate(biomeDict[tempTile.Biome], tileObj.transform.GetChild(0).transform);

                if (tempTile.Biome != Biome.BEACH && !tempTile.water)
                    biomeTile.transform.localScale = new Vector3(0.1f, 1f, 0.1f);

                if (tempTile.water || (tempTile.land && !tempTile.coast))
                {
                    Destroy(tileObj.GetComponent<MeshRenderer>());
                    Destroy(tileObj.GetComponent<MeshFilter>());
                }

                // Place it under Environment
                tileObj.transform.parent = tilesParent.transform;

                // Store the object in Tile class
                MapStats.Instance.tileData[i, j].tileGameObject = tileObj;

                // Spawn theater
                if (theaterPrefab != null && tempTile.isSpawnCenter)
                {
                    GameObject theater = Instantiate(theaterPrefab, new Vector3(tempTile.Center.x, 5.93f, tempTile.Center.y), Quaternion.Euler(0, 180, 0), buildingsParent.transform);
                    theater.GetComponent<BuildingInfo>().BuildingCenter = tempTile.Center;

                    playerTransform = GameObject.Find("Player").transform;
                    // Spawn player in front of theater
                    playerTransform.position = new Vector3(tempTile.Center.x + 7f, 1.55f, tempTile.Center.y - 3f);
                }

                // Spawn items/equipments
                if (theaterPrefab != null && tempTile.hasItemEquipment)
                {
                    GameObject spawnItemEquipment = null;
                    switch (tempTile.ItemEquipment)
                    {
                        case ItemEquipment.Axe:
                            spawnItemEquipment = Instantiate(axePrefab,
                                new Vector3(tempTile.Center.x + Random.Range(-tempTile.HalfLength, tempTile.HalfLength),
                                            1.58f,
                                            tempTile.Center.y + Random.Range(-tempTile.HalfLength, tempTile.HalfLength)),
                                            Quaternion.Euler(0, Random.Range(0, 180), 0), itemsParent.transform);
                            break;
                        case ItemEquipment.Hammer:
                            spawnItemEquipment = Instantiate(hammerPrefab,
                                new Vector3(tempTile.Center.x + Random.Range(-tempTile.HalfLength, tempTile.HalfLength),
                                            1.58f,
                                            tempTile.Center.y + Random.Range(-tempTile.HalfLength, tempTile.HalfLength)),
                                            Quaternion.Euler(0, Random.Range(0, 180), 0), itemsParent.transform);

                            break;
                        case ItemEquipment.Sofa:
                            spawnItemEquipment = Instantiate(sofaPrefab,
                                 new Vector3(tempTile.Center.x + Random.Range(-tempTile.HalfLength, tempTile.HalfLength),
                                             2.5f,
                                             tempTile.Center.y + Random.Range(-tempTile.HalfLength, tempTile.HalfLength)),
                                             Quaternion.Euler(0, Random.Range(0, 180), 0), itemsParent.transform);
                            break;
                        case ItemEquipment.Starbucks:
                            spawnItemEquipment = Instantiate(starbucksPrefab,
                                 new Vector3(tempTile.Center.x + Random.Range(-tempTile.HalfLength, tempTile.HalfLength),
                                             2.785f,
                                             tempTile.Center.y + Random.Range(-tempTile.HalfLength, tempTile.HalfLength)),
                                             Quaternion.Euler(0, Random.Range(0, 180), 0), itemsParent.transform);
                            break;
                        case ItemEquipment.Watermelon:
                            spawnItemEquipment = Instantiate(watermelonPrefab,
                                 new Vector3(tempTile.Center.x + Random.Range(-tempTile.HalfLength, tempTile.HalfLength),
                                             2.423f,
                                             tempTile.Center.y + Random.Range(-tempTile.HalfLength, tempTile.HalfLength)),
                                             Quaternion.Euler(0, Random.Range(0, 180), 0), itemsParent.transform);
                            break;
                    }
                }

                if (theaterPrefab != null && tempTile.hasEnemy)
                {
                    Instantiate(enemyPrefab,
                                new Vector3(tempTile.Center.x + Random.Range(-tempTile.HalfLength, tempTile.HalfLength),
                                            0.58f,
                                            tempTile.Center.y + Random.Range(-tempTile.HalfLength, tempTile.HalfLength)),
                                Quaternion.Euler(0, Random.Range(0, 180), 0), enemiesParent.transform);
                }

                // Generate trees
                if (generateTrees && tempTile.Trees != null)
                {
                    foreach (Vector2 treePos in tempTile.Trees)
                    {
                        GameObject tempTree = Instantiate(treePrefab, new Vector3(tempTile.Center.x + treePos.x, 1f, tempTile.Center.y + treePos.y), Quaternion.identity);
                        tempTree.transform.parent = treesParent.transform;
                    }
                }

                // Generate rocks
                if (generateRocks && tempTile.Rocks != null)
                {
                    foreach (Vector2 rockPos in tempTile.Rocks)
                    {
                        GameObject tempRock = Instantiate(rockVariationPrefabs[Random.Range(0, rockVariationPrefabs.Count)],
                            new Vector3(tempTile.Center.x + rockPos.x, 1f, tempTile.Center.y + rockPos.y), Quaternion.Euler(0, Random.Range(0, 180), 0));
                        tempRock.transform.parent = rocksParent.transform;
                    }

                }
            }
        }
    }

}
