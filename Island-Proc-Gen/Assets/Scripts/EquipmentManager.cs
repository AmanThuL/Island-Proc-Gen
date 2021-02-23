using System.Collections;
using System.Collections.Generic;
using UnityEngine;

public class EquipmentManager : Singleton<EquipmentManager>
{
    public Equipment[] defaultEquipment;
    private Equipment[] currentEquipment;
    private GameObject[] currentMeshes;

    public GameObject targetMesh;

    // Callback for when an item is equipped
    public delegate void OnEquipmentChanged(Equipment newItem, Equipment oldItem);
    public OnEquipmentChanged onEquipmentChanged;

    private Inventory inventory;

    void Start()
    {
        inventory = Inventory.Instance;

        int numSlots = System.Enum.GetNames(typeof(EquipmentSlot)).Length;
        currentEquipment = new Equipment[numSlots];
        currentMeshes = new GameObject[numSlots];

        EquipDefaults();
    }

    void Update()
    {
        if (Input.GetKeyDown(KeyCode.U))
        {
            UnequipAll();
        }
    }

    /// <summary>
    /// Equip a new item
    /// </summary>
    public void Equip(Equipment newItem)
    {
        Equipment oldItem = null;

        // Find out what slot the item fits in
        // and put it there.
        int slotIndex = (int)newItem.equipSlot;

        // If there was already an item in the slot
        // make sure to put it back in the inventory
        if (currentEquipment[slotIndex] != null)
        {
            oldItem = currentEquipment[slotIndex];
            inventory.Add(oldItem);
        }

        // An item has been equipped so we trigger the callback
        if (onEquipmentChanged != null)
        {
            onEquipmentChanged.Invoke(newItem, oldItem);
        }

        currentEquipment[slotIndex] = newItem;
        Debug.Log(newItem.name + " equipped!");

        if (newItem.prefab)
        {
            AttachToMesh(newItem.prefab, slotIndex);
        }
    }

    private void Unequip(int slotIndex)
    {
        if (currentEquipment[slotIndex] != null)
        {
            Equipment oldItem = currentEquipment[slotIndex];
            inventory.Add(oldItem);

            currentEquipment[slotIndex] = null;

            if (onEquipmentChanged != null)
            {
                onEquipmentChanged.Invoke(null, oldItem);
            }
        }
    }

    private void UnequipAll()
    {
        for (int i = 0; i < currentEquipment.Length; i++)
        {
            Unequip(i);
        }
    }

    private void EquipDefaults()
    {
        foreach (Equipment e in defaultEquipment)
        {
            Equip(e);
        }
    }

    private void AttachToMesh(GameObject mesh, int slotIndex)
    {
        if (currentMeshes[slotIndex] != null)
        {
            Destroy(currentMeshes[slotIndex].gameObject);
        }

        // Determine targetMesh based on item type
        switch (slotIndex)
        {
            case (int)EquipmentSlot.Weapon:
                targetMesh = GameObject.Find("WeaponPlaceholder");
                break;
            default:
                targetMesh = GameObject.Find("WeaponPlaceholder");
                break;
        }

        // Attach mesh to corresponding parent
        GameObject newMesh = Instantiate(mesh);
        newMesh.transform.parent = targetMesh.transform;
        newMesh.transform.localPosition = Vector3.zero;
        newMesh.transform.localRotation = Quaternion.identity;
        newMesh.transform.localScale = Vector3.one;
    }
}
