using UnityEngine;

public class CameraController : MonoBehaviour
{
    [Header("Camera Movement")]
    [SerializeField] private float panSpeed = 20f;
    [SerializeField] private float panBorderThickness = 10f;
    // Map boundaries
    [SerializeField] private Vector2 panLimit;
    [SerializeField] private bool isCursorMovementEnabled = false;

    [Header("Camera Zoom")]
    [SerializeField] private float scrollSpeed = 20f;
    [SerializeField] private float minY = 20f;
    [SerializeField] private float maxY = 120f;

    // Update is called once per frame
    void Update()
    {
        Vector3 pos = transform.position;

        if (Input.GetKey("w") || (isCursorMovementEnabled && Input.mousePosition.y >= Screen.height - panBorderThickness))
        {
            pos.z += panSpeed * Time.deltaTime;
        }
        if (Input.GetKey("s") || (isCursorMovementEnabled && Input.mousePosition.y <= panBorderThickness))
        {
            pos.z -= panSpeed * Time.deltaTime;
        }
        if (Input.GetKey("d") || (isCursorMovementEnabled && Input.mousePosition.x >= Screen.width - panBorderThickness))
        {
            pos.x += panSpeed * Time.deltaTime;
        }
        if (Input.GetKey("a") || (isCursorMovementEnabled && Input.mousePosition.x <= panBorderThickness))
        {
            pos.x -= panSpeed * Time.deltaTime;
        }

        // Scrollwheel
        float scroll = Input.GetAxis("Mouse ScrollWheel");
        pos.y -= scroll * scrollSpeed * 100f * Time.deltaTime;

        // Limit camera moveable and zoomable area
        pos.x = Mathf.Clamp(pos.x, -panLimit.x, panLimit.x);
        pos.y = Mathf.Clamp(pos.y, minY, maxY);
        pos.z = Mathf.Clamp(pos.z, -panLimit.y, panLimit.y);

        transform.position = pos;
    }
}
