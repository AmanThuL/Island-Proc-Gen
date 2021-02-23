using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using Assets.Map;
using System;
using System.Linq;
using System.Runtime.InteropServices.WindowsRuntime;

public class Graph
{
    // Fields
    private int width;
    private int height;
    private Func<Vector2, bool> inside;
    private NoiseSettings settings;

    // Poisson Disc Sampling Attributes
    private float poissonRadius;

    public Graph(int _width, int _height, NoiseSettings _settings, float _poissonRadius)
    {
        width = _width;
        height = _height;
        settings = _settings;
        poissonRadius = _poissonRadius;

        IslandShape.SetupIslandShape(_width, _height, _settings);
        inside = IslandShape.makePerlin();

        //GenerateHeightMap(_settings);
    }

    public Tile[,] CreateTerrainTilesData(float tileLength)
    {
        Tile[,] tileData = new Tile[width, height];

        Queue<Tile> borderTiles = new Queue<Tile>();
        List<Tile> waterTiles = new List<Tile>();
        List<Tile> landTiles = new List<Tile>();
        Queue<Tile> lakeTiles = new Queue<Tile>();
        Queue<Tile> beachTiles = new Queue<Tile>();
        Queue<Tile> coastTiles = new Queue<Tile>();
        List<Tile> locations = new List<Tile>();
        List<Tile>[] regions = new List<Tile>[9];

        for (int i = 0; i < width; i++)
        {
            for (int j = 0; j < height; j++)
            {
                // Calculate actual center position in scene
                Vector2 center = new Vector2((-(width - 1) * 0.5f + i) * tileLength, (-(height - 1) * 0.5f + j) * tileLength);

                // Create a tile class
                Tile newTile = new Tile(i, j, center, tileLength);

                // Set property values of each tile
                // 1) Assign borders
                if (i == 0 || i == width - 1 || j == 0 || j == height - 1)
                {
                    newTile.border = true;
                    borderTiles.Enqueue(newTile);
                }

                // 2) Assign water and land
                newTile.water = !inside(newTile.Position);
                newTile.land = !newTile.water;
                if (!newTile.water)
                {
                    newTile.DistanceToWater = int.MaxValue;
                    landTiles.Add(newTile);
                }
                else
                    waterTiles.Add(newTile);

                // Store in 2d array
                tileData[i, j] = newTile;
            }
        }

        // Store neighbor/adjacent tiles
        for (int i = 0; i < width; i++)
        {
            for (int j = 0; j < height; j++)
            {
                tileData[i, j].Neighbors = GetNeighborTiles(tileData[i, j], tileData);
            }
        }

        AssignOceanTiles(borderTiles);
        AssignLakeTiles(waterTiles, lakeTiles);
        AssignBeachAndCoast(landTiles, beachTiles, locations);

        // Elevations
        AssignElevations(beachTiles);
        RedistributeElevations(locations);

        // Moistures
        AssignMoisture(lakeTiles, locations);
        RedistributeMoisture(locations);

        AssignBiome(landTiles);

        // ================================

        // Assign distance to water
        landTiles.ForEach(lt =>
        {
            if (lt.coast)
            {
                lt.DistanceToWater = 1;
                coastTiles.Enqueue(lt);
            }
        });

        while (coastTiles.Count > 0)
        {
            Tile t = coastTiles.Dequeue();

            foreach (Tile n in t.Neighbors)
            {
                if (!n.land) continue;
                int newD = t.DistanceToWater + 1;
                if (newD < n.DistanceToWater)
                {
                    n.DistanceToWater = newD;
                    coastTiles.Enqueue(n);
                }
            }
        }

        // Assign region
        foreach (Tile lt in landTiles)
        {
            AssignRegion(lt);
            if (regions[(int)lt.RegionNum] == null)
                regions[(int)lt.RegionNum] = new List<Tile>();
            regions[(int)lt.RegionNum].Add(lt);
        }

        // Find a spawn region for player and theater
        // The region with the most land tiles excluding the middle region
        Region spawnRegion = Region.Northwest;
        for (int i = 0; i < regions.Length; i++)
        {
            if ((Region)i == Region.Middle) continue;
            int landTilesCount = regions[i].Count;
            //Debug.Log("Region " + ((Region)i).ToString() + " has " + landTilesCount + " land tiles");
            if (landTilesCount > regions[(int)spawnRegion].Count)
            {
                spawnRegion = (Region)i;
            }
        }

        Debug.Log("Spawn region is " + spawnRegion.ToString());

        // Determine spawn center of the theater
        // It needs to be at the farthest distance from water
        Tile potentialSpawnTile = null;
        foreach (Tile rt in regions[(int)spawnRegion])
        {
            if (potentialSpawnTile == null || rt.DistanceToWater > potentialSpawnTile.DistanceToWater)
            {
                potentialSpawnTile = rt;
                if (rt.DistanceToWater == 7) break;
            }
        }
        potentialSpawnTile.isSpawnCenter = true;
        for (int i = -4; i <= 4; i++)
        {
            for (int j = -4; j <= 4 ; j++)
            {
                Vector2Int pos = new Vector2Int(potentialSpawnTile.Position.x + i, potentialSpawnTile.Position.y + j);
                tileData[pos.x, pos.y].hasBuilding = true;
            }
        }

        Debug.Log("Potential spawn tile is " + potentialSpawnTile.Position 
            + " with " + potentialSpawnTile.DistanceToWater + " units of distance to nearest water.");


        // Spawn items
        List<Tile> watermelonTileList = new List<Tile>();
        List<Tile> starbucksTileList = new List<Tile>();
        List<Tile> sofaTileList = new List<Tile>();
        for (int i = 0; i < regions.Length; i++)
        {
            if ((Region)i == spawnRegion) continue;

            SpawnItemEquipment(regions[i], ItemEquipment.Sofa, 0.2f, sofaTileList);
            SpawnItemEquipment(regions[i], ItemEquipment.Hammer, 0.3f);
            SpawnItemEquipment(regions[i], ItemEquipment.Starbucks, 0.4f, starbucksTileList);
            SpawnItemEquipment(regions[i], ItemEquipment.Watermelon, 0.6f, watermelonTileList);

            if (i == regions.Length - 1)
            {
                if (watermelonTileList.Count == 0)
                    SpawnItemEquipment(regions[i], ItemEquipment.Watermelon, 1f, watermelonTileList);
                if (starbucksTileList.Count == 0)
                    SpawnItemEquipment(regions[i], ItemEquipment.Starbucks, 1f, starbucksTileList);
                if (sofaTileList.Count == 0)
                    SpawnItemEquipment(regions[i], ItemEquipment.Sofa, 1f, sofaTileList);
            }
        }

        // Spawn enemies
        foreach (Tile watermelonTile in watermelonTileList)
        {
            SpawnEnemiesAroundItem(watermelonTile, 0.2f);
        }
        foreach (Tile starbucksTile in starbucksTileList)
        {
            SpawnEnemiesAroundItem(starbucksTile, 0.3f);
        }
        foreach (Tile sofaTile in sofaTileList)
        {
            SpawnEnemiesAroundItem(sofaTile, 0.3f);
        }

        // Trees
        foreach (Tile t in locations)
        {
            if (t.hasBuilding || t.hasItemEquipment) continue;
            List<Vector2> trees = new PoissonDiscSampling(poissonRadius, new Vector2(tileLength, tileLength), GetRejectionSamples(t.Biome)).SamplingPoints;

            List<Tile> lakeNeighbors = new List<Tile>();
            t.Neighbors.ForEach(nb =>
            {
                if (nb.lake)
                    lakeNeighbors.Add(nb);
            });

            if (lakeNeighbors.Count > 0)
            {
                foreach (Tile lake in lakeNeighbors)
                {
                    for (int i = 0; i < trees.Count; i++)
                    {
                        Vector2 newTreeCenter = t.Center + trees[i];
                        if (lake.IsWithinTile(newTreeCenter))
                            trees.RemoveAt(i);
                    }
                }
            }

            t.Trees = trees;
        }

        // Rocks
        foreach (Tile t in locations)
        {
            if (t.hasBuilding || t.hasItemEquipment) continue;
            if (t.Biome != Biome.DESERT && t.Biome != Biome.TEMPERATE_DESERT) continue;

            t.Rocks = new List<Vector2>();

            // First rock
            if (RandomSelect_Percentage(0.1f))
                t.Rocks.Add(new Vector2(UnityEngine.Random.Range(-t.Length / 2f, t.Length / 2f), UnityEngine.Random.Range(-t.Length / 2f, t.Length / 2f)));

            // Second rock
            if (RandomSelect_Percentage(0.05f))
                t.Rocks.Add(new Vector2(UnityEngine.Random.Range(-t.Length / 2f, t.Length / 2f), UnityEngine.Random.Range(-t.Length / 2f, t.Length / 2f)));
        }

        return tileData;
    }

    // =================================================================================

    private void AssignRegion(Tile tile)
    {
        double oneThirdWidth = width / 3f;
        double oneThirdHeight = height / 3f;

        int regionNum = (int)Mathf.Clamp((float)Math.Floor(tile.Position.y / oneThirdHeight), 0, 2) * 3
                      + (int)Mathf.Clamp((float)Math.Floor(tile.Position.x / oneThirdWidth), 0, 2);
        tile.RegionNum = (Region)regionNum;
    }

    /// <summary>
    /// Assign each land tile with matching biome.
    /// </summary>
    private void AssignBiome(List<Tile> landTiles)
    {
        foreach (Tile t in landTiles)
        {
            if (t.Biome == Biome.BEACH) continue;
            else
                t.Biome = GetBiome(t.Elevation, t.Moisture);
            //t.Biome = Biome.GRASSLAND;
        }

    }

    /// <summary>
    /// Change the overall distribution of moisture to be evenly distributed.
    /// </summary>
    private void RedistributeMoisture(List<Tile> locations)
    {
        List<Tile> sortedLandTiles = locations.OrderBy(t => t.Moisture).ToList();
        for (int i = 0; i < sortedLandTiles.Count; i++)
        {
            sortedLandTiles[i].Moisture = (float)i / (sortedLandTiles.Count - 1);
        }
    }

    /// <summary>
    /// Determine moisture for each land tiles starting with 1 for all lake tiles.
    /// </summary>
    private void AssignMoisture(Queue<Tile> lakeTiles, List<Tile> locations)
    {
        // Assign moisture
        float halfWidth = width / 2f;
        float halfHeight = height / 2f;

        foreach (Tile t in locations)
        {
            Vector2 q = new Vector2(t.Position.x / halfWidth - 1, t.Position.y / halfHeight - 1);
            float x = (q.x + IslandShape.offset.x) * settings.scale * settings.moistureDistributionRatio;
            float y = (q.y + IslandShape.offset.y) * settings.scale * settings.moistureDistributionRatio;
            t.Moisture = Mathf.PerlinNoise(x, y);
        }

        while (lakeTiles.Count > 0)
        {
            Tile t = lakeTiles.Dequeue();

            foreach (Tile n in t.Neighbors)
            {
                if (n.ocean || n.Biome == Biome.BEACH) continue;
                float newMoisture = t.Moisture * 0.95f;
                if (newMoisture > n.Moisture)
                {
                    n.Moisture = newMoisture;
                    lakeTiles.Enqueue(n);
                }
            }
        }
    }

    /// <summary>
    /// Rescale elevations so that the highest is 1.0, and they're distributed well.
    /// </summary>
    private void RedistributeElevations(List<Tile> locations)
    {
        // SCALE_FACTOR increases the mountain area. At 1.0 the maximum
        // elevation barely shows up on the map, so we set it to 1.1.
        float SCALE_FACTOR = 1.1f;
        float x = 0, y = 0;

        List<Tile> sortedLandTiles = locations.OrderBy(t => t.Elevation).ToList();


        for (int i = 0; i < sortedLandTiles.Count; i++)
        {
            // Let y(x) be the total area that we want at elevation <= x.
            // We want the higher elevations to occur less than lower
            // ones, and set the area to be y(x) = 1 - (1-x)^2.
            y = (float)i / (sortedLandTiles.Count - 1);

            // Now we have to solve for x, given the known y.
            //  *  y = 1 - (1-x)^2
            //  *  y = 1 - (1 - 2x + x^2)
            //  *  y = 2x - x^2
            //  *  x^2 - 2x + y = 0
            // From this we can use the quadratic equation to get:
            x = Mathf.Sqrt(SCALE_FACTOR) - Mathf.Sqrt(SCALE_FACTOR * (1 - y));
            if (x > 1f) x = 1f;
            sortedLandTiles[i].Elevation = x;
        }
    }

    /// <summary>
    /// Determine elevations for each land tiles starting with 0 for all beaches.
    /// </summary>
    private void AssignElevations(Queue<Tile> beachTiles)
    {
        while (beachTiles.Count > 0)
        {
            Tile t = beachTiles.Dequeue();

            foreach (Tile n in t.Neighbors)
            {
                float newElevation = 0.01f + t.Elevation;
                if (t.land && n.land)
                {
                    newElevation += 1;
                }
                // If this point changed, we'll add it to the queue so that
                // we can process its neighbors too
                if (newElevation < n.Elevation)
                {
                    n.Elevation = newElevation;
                    beachTiles.Enqueue(n);
                }
            }
        }
    }

    /// <summary>
    /// Assign coast (ocean/lake adjacent) and beach (ocean adjacent only) tiles.
    /// </summary>
    private void AssignBeachAndCoast(List<Tile> landTiles, Queue<Tile> beachTiles, List<Tile> locations)
    {
        foreach (Tile t in landTiles)
        {
            t.Moisture = 0f;
            t.Elevation = float.MaxValue;
            foreach (Tile n in t.Neighbors)
            {
                if (n.water || t.border)
                {
                    if (n.ocean || t.border)
                    {
                        t.Biome = Biome.BEACH;
                        t.Elevation = 0f;
                        beachTiles.Enqueue(t);
                    }
                    t.coast = true;
                    break;
                }
            }
        }

        // Add land tiles excluding beach tiles to locations array
        foreach (Tile t in landTiles)
        {
            if (t.Biome == Biome.BEACH) continue;
            locations.Add(t);
        }

    }

    /// <summary>
    /// Separate lake tiles from water tiles and add them to queue for later use.
    /// </summary>
    private void AssignLakeTiles(List<Tile> waterTiles, Queue<Tile> lakeTiles)
    {
        foreach (Tile t in waterTiles)
        {
            if (!t.ocean)
            {
                t.lake = true;
                t.Biome = Biome.LAKE;
                t.Moisture = 1f;

                // For moisture calculation use
                lakeTiles.Enqueue(t);
            }
        }
    }

    /// <summary>
    /// DFS to search for all ocean tiles.
    /// </summary>
    private void AssignOceanTiles(Queue<Tile> borderTiles)
    {
        while (borderTiles.Count > 0)
        {
            Tile t = borderTiles.Dequeue();
            foreach (Tile neighbor in t.Neighbors)
            {
                if (neighbor.water && !neighbor.ocean)
                {
                    neighbor.ocean = true;
                    borderTiles.Enqueue(neighbor);
                }
            }
        }
    }

    private void SpawnItemEquipment(List<Tile> regionList, ItemEquipment type, float percentage = 0.5f, List<Tile> tileList = null)
    {
        bool spawn = RandomSelect_Percentage(percentage);
        if (spawn)
        {
            Tile chosenTile = regionList[UnityEngine.Random.Range(0, regionList.Count)];
            if (chosenTile.hasEnemy || chosenTile.hasItemEquipment) return;
            chosenTile.hasItemEquipment = true;
            chosenTile.ItemEquipment = type;

            if (tileList != null)
                tileList.Add(chosenTile);
        }
    }

    private void SpawnEnemiesAroundItem(Tile itemTile, float enemySpawnProb)
    {
        List<Tile> neighborTiles = new List<Tile>();
        foreach (Tile n1 in itemTile.Neighbors)
        {
            if (n1.hasItemEquipment || n1.hasBuilding) continue;
            if (n1.land) neighborTiles.Add(n1);
            foreach (Tile n2 in n1.Neighbors)
            {
                if (n2.hasItemEquipment || n2.hasBuilding) continue;
                if (!neighborTiles.Contains(n1) && n2.land)
                    neighborTiles.Add(n2);
            }
        }

        foreach (Tile n in neighborTiles)
        {
            bool spawnOrNot = RandomSelect_Percentage(enemySpawnProb);
            if (spawnOrNot)
            {
                n.hasEnemy = true;
            }
        }
    }

    #region Utility Methods

    /// <summary>
    /// Return all valid 4 neighbors of a given tile (manhattan)
    /// </summary>
    private List<Tile> Get4NeighborTiles(Tile newTile, Tile[,] tileData)
    {
        List<Tile> neighbors = new List<Tile>();
        int x = (int)newTile.Position.x;
        int y = (int)newTile.Position.y;

        if (x - 1 >= 0) neighbors.Add(tileData[x - 1, y]);
        if (x + 1 < width) neighbors.Add(tileData[x + 1, y]);
        if (y - 1 >= 0) neighbors.Add(tileData[x, y - 1]);
        if (y + 1 < height) neighbors.Add(tileData[x, y + 1]);

        return neighbors;
    }

    /// <summary>
    /// Return all valid neighbors of a given tile (euclidean)
    /// </summary>
    private List<Tile> GetNeighborTiles(Tile newTile, Tile[,] tileData)
    {
        List<Tile> neighbors = new List<Tile>();
        int x = (int)newTile.Position.x;
        int y = (int)newTile.Position.y;

        int xMin = x - 1 >= 0 ? x - 1 : x;
        int xMax = x + 1 < width ? x + 1 : x;
        int yMin = y - 1 >= 0 ? y - 1 : y;
        int yMax = y + 1 < height ? y + 1 : y;

        // 1 2 3
        // 4 T 5
        // 6 7 8
        for (int i = xMin; i <= xMax; i++)
        {
            for (int j = yMin; j <= yMax; j++)
            {
                if (i == x && j == x) continue;
                neighbors.Add(tileData[i, j]);
            }
        }

        return neighbors;
    }

    private Biome GetBiome(float e, float m)
    {
        if (e > 0.4f)
        {
            if (m > 0.83f) return Biome.RAIN_FOREST;
            else if (m > 0.50f) return Biome.FOREST;
            else if (m > 0.16f) return Biome.GRASSLAND;
            else return Biome.TEMPERATE_DESERT;
        }
        else
        {
            if (m > 0.67f) return Biome.RAIN_FOREST;
            else if (m > 0.33f) return Biome.FOREST;
            else if (m > 0.16f) return Biome.GRASSLAND;
            else return Biome.DESERT;
        }
    }

    private int GetRejectionSamples(Biome biome)
    {
        switch (biome)
        {
            case Biome.DESERT: return 0;
            case Biome.FOREST: return 3;
            case Biome.GRASSLAND: return 1;
            case Biome.RAIN_FOREST: return 3;
            case Biome.TEMPERATE_DESERT: return 1;
        }
        return 0;
    }

    private bool RandomSelect_Percentage(float prob)
    {
        float checkValue = UnityEngine.Random.value;

        return checkValue <= prob ? true : false;
    }

    #endregion
}
