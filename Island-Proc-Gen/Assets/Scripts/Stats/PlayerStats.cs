using System.Collections;
using System.Collections.Generic;
using UnityEngine;

public class PlayerStats : CharacterStats
{
    // Start is called before the first frame update
    public override void Start()
    {
        base.Start();
        EquipmentManager.Instance.onEquipmentChanged += OnEquipmentChanged;
    }

    private void OnEquipmentChanged(Equipment newItem, Equipment oldItem)
    {
        if (newItem != null)
        {
            armor.AddModifier(newItem.armorModifier);
            damage.AddModifier(newItem.damageModifer);
        }

        if (oldItem != null)
        {
            armor.RemoveModifier(oldItem.armorModifier);
            damage.RemoveModifier(oldItem.damageModifer);
        }
    }

    public override void Die()
    {
        base.Die();

        WinLossManager.Instance.Die();
        // Kill the player
        //PlayerManager.Instance.KillPlayer();
    }
}
