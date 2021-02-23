using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.AI;

public class TheaterController : MonoBehaviour
{
    [SerializeField]
    private GameObject watermelonPrefab;
    [SerializeField]
    private GameObject starbucksPrefab;
    [SerializeField]
    private GameObject sofaPrefab;

    [SerializeField]
    private NavMeshObstacle navMeshBox;

    private Vector3 actualCenter;
    [SerializeField]
    [Range(0, 30f)] private float effectiveRange = 5f;

    private bool hasWatermelon, hasStarbucks, hasSofa;

    // Start is called before the first frame update
    void Start()
    {
        actualCenter = transform.position;
        hasWatermelon = hasSofa = hasStarbucks = false;
    }

    void Update()
    {
        if (hasWatermelon && hasSofa && hasStarbucks)
        {
            WinLossManager.Instance.Win();
        }
    }

    public void ActivateItem(string itemName)
    {
        switch (itemName)
        {
            case "Watermelon":
                watermelonPrefab.SetActive(true);
                hasWatermelon = true;
                break;
            case "Starbucks":
                starbucksPrefab.SetActive(true);
                hasStarbucks = true;
                break;
            case "Sofa":
                sofaPrefab.SetActive(true);
                hasSofa = true;
                break;
        }
    }

    public bool IsWithinActivationRange(Transform player)
    {
        return Vector3.Distance(player.position, actualCenter) < effectiveRange;
    }

    private void OnDrawGizmosSelected()
    {
        Gizmos.color = Color.green;
        Gizmos.DrawWireSphere(actualCenter, effectiveRange);
    }
}
