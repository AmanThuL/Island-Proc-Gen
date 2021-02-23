using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UI;

public class MinimapDirection : MonoBehaviour
{
    [SerializeField]
    private Transform playerTransform;

    private RectTransform minimapBorderRect;

    // Start is called before the first frame update
    void Start()
    {
        minimapBorderRect = GetComponent<RectTransform>();
    }

    // Update is called once per frame
    void Update()
    {
        minimapBorderRect.localRotation = Quaternion.Euler(0, 0, playerTransform.localRotation.eulerAngles.y + 90);
    }
}
