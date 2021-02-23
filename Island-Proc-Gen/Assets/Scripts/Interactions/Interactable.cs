using UnityEngine;

/// <summary>
/// This component is for all objects that the player can
/// interact with such as enemies, items, etc. It is meant
/// to be used as a base class.
/// </summary>
public class Interactable : MonoBehaviour
{
    public float radius = 3f;
    public Transform interactionTransform;

    private bool isFocus = false;       // Is this interactable currently being focused?
    private Transform player;           // Reference to the player transform

    private bool hasInteracted = false; // Have we already interacted with the object?

    void Update()
    {
        // If we are currently being focused
        // and we haven't already interacted with the object
        if (isFocus && !hasInteracted)
        {
            // If we are close enough
            float distance = Vector3.Distance(player.position, interactionTransform.position);
            if (distance <= radius)
            {
                // Interact with the object
                hasInteracted = true;
                Interact();
            }
        }
    }

    /// <summary>
    /// Called when the object starts being focused.
    /// </summary>
    public void OnFocused(Transform playerTransform)
    {
        isFocus = true;
        player = playerTransform;
        hasInteracted = false;
    }

    /// <summary>
    /// Called when the object is no longer focused.
    /// </summary>
    public void OnDefocused()
    {
        isFocus = false;
        player = null;
        hasInteracted = false;
    }

    /// <summary>
    /// This method is meant to be overwritten.
    /// </summary>
    public virtual void Interact()
    {
        // This method is meant to be overwritten
        //Debug.Log("Interacting with " + transform.name);
    }

    private void OnDrawGizmosSelected()
    {
        if (interactionTransform == null)
        {
            interactionTransform = transform;
        }

        Gizmos.color = Color.yellow;
        Gizmos.DrawWireSphere(interactionTransform.position, radius);
    }
}
