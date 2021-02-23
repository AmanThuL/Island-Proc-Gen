using System.Collections;
using System.Collections.Generic;
using UnityEngine;

/// <summary>
/// An Item that can be equipped to increase armor/damage.
/// </summary>
[CreateAssetMenu(fileName = "New Equipment", menuName = "Inventory/Equipment")]
public class Equipment : Item
{
    public EquipmentSlot equipSlot; // What slot to equip it in

    public int armorModifier;       // Increase/decrease in armor
    public int damageModifer;       // Increase/decrease in damage
    public GameObject prefab;

    public override void Use()
    {
        //base.Use();

        EquipmentManager.Instance.Equip(this);  // Equip it
        RemoveFromInventory();                  // Remove it from inventory
    }
}

public enum EquipmentSlot
{
    Head,
    Chest,
    Legs,
    Weapon,
    Shield,
    Feet
}