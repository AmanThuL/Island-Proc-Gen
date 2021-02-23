using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using Assets.Map;

public class MapStats : Singleton<MapStats>
{
    #region Fields

    public int seed;
    public Tile[,] tileData;

    public int mapWidth, mapHeight;
    public float tileLength;
    public Mesh oceanMesh;

    #endregion

    #region Methods

    public Tile GetNearestTile(Vector3 hitPos, Vector2 sizeInTiles)
    {
        float hitPosX = hitPos.x;
        float hitPosY = hitPos.z;

        int x = (int)((hitPosX - (-mapWidth * 0.5f * tileLength)) / tileLength);
        int y = (int)((hitPosY - (-mapHeight * 0.5f * tileLength)) / tileLength);

        // If x or y is out of bound
        if (x < 0 || x >= tileData.GetLength(0) || y < 0 || y >= tileData.GetLength(1))
            return null;

        // If tile is on a non-water biome
        if (tileData[x, y].water)
            return null;

        // Check if the tiles within the building range has building
        int halfX = (int)sizeInTiles.x / 2;
        int halfY = (int)sizeInTiles.y / 2;
        for (int i = -halfX; i <= halfX; i++)
        {
            for (int j = -halfY; j <= halfY; j++)
            {
                if (tileData[x + i, y + j].hasBuilding)
                    return null;
            }
        }

        // For now, only return a tile if the hitPos falls on a non-water biome tile
        return tileData[x, y];
    }

    public void StoreTilesBuildingInfo(Vector2 position, BuildingInfo buildingInfo)
    {
        int halfX = (int)buildingInfo.BuildingSizeInTiles.x / 2;
        int halfY = (int)buildingInfo.BuildingSizeInTiles.y / 2;

        for (int i = -halfX; i <= halfX; i++)
        {
            for (int j = -halfY; j <= halfY; j++)
            {
                int tempX = (int)position.x + i, tempY = (int)position.y + j;
                if (tempX < 0 || tempX > tileData.GetLength(0) || tempY < 0 || tempY > tileData.GetLength(1))
                    continue;
                tileData[tempX, tempY].hasBuilding = true;
                tileData[tempX, tempY].BuildingInfo = buildingInfo;
            }
        }        
    }

    #endregion
}
