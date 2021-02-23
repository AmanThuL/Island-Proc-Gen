using System.Collections;
using System.Collections.Generic;
using UnityEngine;

public class MouseClickEffect : MonoBehaviour
{
    [SerializeField]
    private GameObject neonCircle;
    [SerializeField]
    [Range(0f, 3f)] private float emitDuration;

    [SerializeField]
    private Vector3 originalScale;
    [SerializeField]
    private Vector3 destinationScale;

    // Start is called before the first frame update
    void Start()
    {
        StartCoroutine(EmitVFXOverTime());
    }


    private IEnumerator EmitVFXOverTime()
    {
        float currentTime = 0.0f;
        GameObject vfxObj = Instantiate(neonCircle, gameObject.transform);

        do
        {
            vfxObj.transform.localScale = Vector3.Lerp(originalScale, destinationScale, currentTime / emitDuration);

            currentTime += Time.deltaTime;
            yield return null;
        } while (currentTime <= emitDuration);

        Destroy(vfxObj);
        Destroy(gameObject);
    }
}
