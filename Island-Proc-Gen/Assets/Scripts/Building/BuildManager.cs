using Assets.Map;
using System;
using System.Collections;
using System.Collections.Generic;
using UnityEngine;

public enum BuildingType
{
    Castle,
    Warehouse,
    Market,
    Farm
}

public class BuildManager : MonoBehaviour
{
    [Header("Prefabs")]
    [SerializeField]
    private GameObject placeableObjectPrefab;
    [SerializeField]
    private GameObject tileSelectorPrefab;

    [Header("Buildings Dictionary")]
    [SerializeField]
    private BuildingTypeGameObjectDictionary buildingTypeDict;

    [Header("Hotkeys")]
    [SerializeField]
    private KeyCode rotatePlaceableObjectHotkey = KeyCode.R;

    [Header("Materials")]
    [SerializeField]
    private Material successBuildMat;
    [SerializeField]
    private Material failBuildMat;

    [Header("Misc")]
    [SerializeField]
    private LayerMask tileLayerMask;

    // Private variables
    private GameObject currentPlaceableObject;
    private GameObject tileSelector;
    private Material originalPlaceableObjectMat;
    private bool isPlaceableNow;

    private Ray ray;
    private RaycastHit hit;
    private BuildingInfo buildingInfo;
    private Tile selectedTile;

    private void Awake()
    {
        tileSelector = null;
    }

    // Start is called before the first frame update
    void Start()
    {
        isPlaceableNow = false;

        tileSelector = Instantiate(tileSelectorPrefab);
        tileSelector.SetActive(false);

    }

    // Update is called once per frame
    void Update()
    {
        ray = Camera.main.ScreenPointToRay(Input.mousePosition);

        if (currentPlaceableObject != null)
        {
            MoveCurrentPlaceableObjectToMouse();
            RotatePlaceableObjectFromHotkey();
            ReleaseIfClicked();
        }
        else
        {
            // When not building
            MoveTileSelector();
        }
    }


    // Utility methods
    private void MoveTileSelector()
    {
        if (Physics.Raycast(ray, out hit, tileLayerMask))
        {
            Tile selectedTile = MapStats.Instance.GetNearestTile(hit.point, Vector2.one);

            if (selectedTile == null) return;

            // Activate tile selector
            tileSelector.SetActive(true);
            tileSelector.transform.position = new Vector3(selectedTile.Center.x, hit.point.y, selectedTile.Center.y); ;
        }
    }

    /// <summary>
    /// When player presses any building button, create a matching game object
    /// </summary>
    public void HandleNewObjectButton(BuildingType buildingType)
    {
        if (currentPlaceableObject == null)
        {
            placeableObjectPrefab = buildingTypeDict[buildingType];
            currentPlaceableObject = Instantiate(placeableObjectPrefab);

            // Store original material
            originalPlaceableObjectMat = placeableObjectPrefab.GetComponent<MeshRenderer>().sharedMaterial;
        }
        else
        {
            Destroy(currentPlaceableObject);
        }
    }

    /// <summary>
    /// Move new created object to a position on the grid that is closest to the mouse position
    /// </summary>
    private void MoveCurrentPlaceableObjectToMouse()
    {
        if (Physics.Raycast(ray, out hit))
        {
            PlaceObjectNear(hit.point);
            //Debug.Log("Hit point is " + hit.point);
        }
    }

    /// <summary>
    /// Press the rotation hotkey to rotate the build object
    /// </summary>
    private void RotatePlaceableObjectFromHotkey()
    {
        if (Input.GetKeyDown(rotatePlaceableObjectHotkey))
        {
            currentPlaceableObject.transform.Rotate(Vector3.up, 90f);
        }
    }

    /// <summary>
    /// Place the build object to the position when player presses the left mouse button
    /// </summary>
    private void ReleaseIfClicked()
    {
        if (Input.GetMouseButtonDown(0) && isPlaceableNow)
        {
            // Deactivate tile selector
            tileSelector.SetActive(false);
            ScaleTileSelector(1, 1);

            // Assign original mat to the object
            currentPlaceableObject.GetComponent<MeshRenderer>().material = originalPlaceableObjectMat;

            currentPlaceableObject = null;

            // Update tileData!
            MapStats.Instance.StoreTilesBuildingInfo(selectedTile.Position, buildingInfo);
        }
    }

    /// <summary>
    /// Check the position and display if the current position is valid for placement
    /// </summary>
    private void DisplayBuildValidness(bool isBuildValid)
    {
        isPlaceableNow = isBuildValid;

        // Assign success/fail build mat to object
        currentPlaceableObject.GetComponent<MeshRenderer>().material = isBuildValid ? successBuildMat : failBuildMat;
    }

    /// <summary>
    /// Snap the placement position to the nearest tile
    /// </summary>
    private void PlaceObjectNear(Vector3 hitPos)
    {
        // Get info of current placeable object
        buildingInfo = currentPlaceableObject.GetComponent<BuildingInfo>();

        ScaleBuildingModel(buildingInfo.BuildingSizeInTiles.x * MapStats.Instance.tileLength / buildingInfo.BuildingModelSize.x);

        selectedTile = MapStats.Instance.GetNearestTile(hitPos, buildingInfo.BuildingSizeInTiles);

        bool isBuildValid;
        Vector3 finalPosition;

        if (selectedTile != null)
        {
            isBuildValid = true;
            DisplayBuildValidness(isBuildValid);
            finalPosition = new Vector3(selectedTile.Center.x, hitPos.y, selectedTile.Center.y);

            // Activate tile selector
            tileSelector.SetActive(true);
            tileSelector.transform.position = finalPosition;
            ScaleTileSelector((int)buildingInfo.BuildingSizeInTiles.x, (int)buildingInfo.BuildingSizeInTiles.y);
        }
        else
        {
            isBuildValid = false;
            DisplayBuildValidness(isBuildValid);
            finalPosition = hitPos;

            // Deactivate tile selector
            tileSelector.SetActive(false);
        }

        currentPlaceableObject.transform.position = finalPosition;
    }

    private void ScaleTileSelector(int scaleRatioX, int scaleRatioY)
    {
        tileSelector.transform.localScale = new Vector3(scaleRatioX, 1, scaleRatioY);
    }

    private void ScaleBuildingModel(float scaleRatio)
    {
        currentPlaceableObject.transform.localScale *= scaleRatio;
    }
}
