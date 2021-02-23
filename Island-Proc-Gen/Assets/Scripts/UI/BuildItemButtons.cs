using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UI;
using System;

public class BuildItemButtons : MonoBehaviour
{
    [SerializeField]
    private List<Button> buildItemButtons;

    private void Awake()
    {        
        // Add all child buttons to list
        buildItemButtons = new List<Button>();
        for (int i = 0; i < transform.childCount; i++)
        {
            buildItemButtons.Add(transform.GetChild(i).gameObject.GetComponent<Button>());
        }

        BuildManager buildManager = GameObject.Find("Build Manager").GetComponent<BuildManager>();

        foreach (Button buildItemButton in buildItemButtons)
        {
            string buttonName = buildItemButton.name;
            buildItemButton.onClick.AddListener(() => buildManager.HandleNewObjectButton((BuildingType)Enum.Parse(typeof(BuildingType), buttonName)));
        }
    }

    // Start is called before the first frame update
    void Start()
    {
        
    }

    // Update is called once per frame
    void Update()
    {
        
    }
}
