using System.Collections;
using System.Collections.Generic;
using UnityEngine;

public class BuildingInfo : MonoBehaviour
{
    [Header("General Properties")]
    [SerializeField]
    private Vector2 buildingModelSize;  // In tile unit length
    [SerializeField]
    private Vector2 buildingSizeInTiles;    // How many tiles?
    [SerializeField]
    private Vector2 buildingCenter;         // Center of building in world coordinates

    // Properties
    public Vector2 BuildingModelSize { get => buildingModelSize; }
    public Vector2 BuildingSizeInTiles { get => buildingSizeInTiles; }
    public Vector2 BuildingCenter { get => buildingCenter; set => buildingCenter = value; }


}
