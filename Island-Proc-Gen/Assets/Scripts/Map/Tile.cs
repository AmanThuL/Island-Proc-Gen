using System.Collections;
using System.Collections.Generic;
using UnityEngine;

namespace Assets.Map
{
    public class Tile
    {
        // Booleans
        public bool border;

        public bool water, land;

        public bool coast, ocean, lake;

        public bool hasBuilding = false;

        public bool hasItemEquipment = false;

        public bool hasEnemy = false;

        public bool isSpawnCenter = false;

        // Properties
        public Vector2Int Position { get; private set; }   // Position in 2d array
        public Vector2 Center { get; private set; }     // Actual position in scene

        public Region RegionNum { get; set; }           // Region number
        public Biome Biome { get; set; }                // biome type
        public ItemEquipment ItemEquipment { get; set; }
        public float Elevation { get; set; }            // 0.0 - 1.0
        public float Moisture { get; set; }             // 0.0 - 1.0
        public int DistanceToWater { get; set; }      // Closest distance to water (ocean, lake)
        public BuildingInfo BuildingInfo { get; set; }
        public List<Tile> Neighbors { get; set; }
        public List<Vector2> Trees { get; set; }
        public List<Vector2> Rocks { get; set; }

        // GameObject
        public GameObject tileGameObject;               // Actual instantiated object in scene

        public float Length { get; private set; }       // Side length of a tile
        public float HalfLength { get; private set; }

        public Tile(int x, int y, Vector2 _center, float _length)
        {
            Position = new Vector2Int(x, y);
            Center = _center;
            Length = _length;
            HalfLength = _length / 2f;
        }

        public bool IsWithinTile(Vector2 pos)
        {
            if (pos.x <= Center.x + HalfLength &&
                pos.x >= Center.x - HalfLength &&
                pos.y <= Center.y + HalfLength &&
                pos.y >= Center.y - HalfLength)
                return true;
            return false;
        }
    }
}
