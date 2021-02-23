using System;
using System.Collections;
using System.Collections.Generic;
using System.Security.Cryptography;
using UnityEngine;
using UnityEngine.EventSystems;

[RequireComponent(typeof(PlayerMotor))]
public class PlayerController : MonoBehaviour
{
    [SerializeField]
    private LayerMask groundLayerMask;
    [SerializeField]
    private LayerMask pickupLayerMask;

    // Private variables
    private Camera cam;
    private PlayerMotor motor;

    [SerializeField]
    private Interactable focus;

    [SerializeField]
    private GameObject mouseClickEffect;

    void Awake()
    {
        cam = Camera.main;
        motor = GetComponent<PlayerMotor>();
    }

    void Start()
    {

    }

    void Update()
    {
        if (EventSystem.current.IsPointerOverGameObject())
            return;

        if (Input.GetMouseButtonDown(0))
        {
            Ray ray = cam.ScreenPointToRay(Input.mousePosition);
            RaycastHit hit;

            if (Physics.Raycast(ray, out hit, 100, groundLayerMask))
            {
                //Debug.Log("Raycast hit at " + hit.point);
                Instantiate(mouseClickEffect, new Vector3(hit.point.x, hit.point.y + 0.1f, hit.point.z), Quaternion.identity);

                motor.MoveToPoint(hit.point);

                RemoveFocus();
            }
        }

        if (Input.GetMouseButtonDown(1))
        {
            Ray ray = cam.ScreenPointToRay(Input.mousePosition);
            RaycastHit hit;

            if (Physics.Raycast(ray, out hit, 100))
            {
                // Check if we hit an interactable
                Interactable interactable = hit.collider.GetComponent<Interactable>();

                if (interactable != null)
                {
                    //Debug.Log("Item hit at " + hit.point);

                    SetFocus(interactable);
                }
            }
        }
    }

    private void SetFocus(Interactable newFocus)
    {
        if (newFocus != focus)
        {
            if (focus != null)
                focus.OnDefocused();
            focus = newFocus; 
            motor.FollowTarget(newFocus);
        }

        newFocus.OnFocused(transform);
    }

    private void RemoveFocus()
    {
        if (focus != null)
            focus.OnDefocused();

        focus = null;
        motor.StopFollowingTarget();
    }

    #region Utility Methods


    #endregion
}
