using System.Collections;
using System.Collections.Generic;
using UnityEngine;

public class PlayerAnimator : CharacterAnimator
{
    public WeaponAnimations[] weaponAnimations;
    private Dictionary<Equipment, AnimationClip[]> weaponAnimationDict;

    protected override void Start()
    {
        base.Start();
        EquipmentManager.Instance.onEquipmentChanged += OnEquipmentChanged;

        weaponAnimationDict = new Dictionary<Equipment, AnimationClip[]>();
        foreach (WeaponAnimations a in weaponAnimations)
        {
            weaponAnimationDict.Add(a.weapon, a.clips);
        }
    }

    protected override void Update()
    {
        base.Update();
    }

    void OnEquipmentChanged(Equipment newItem, Equipment oldItem)
    {
        if (newItem != null && newItem.equipSlot == EquipmentSlot.Weapon)
        {
            if (weaponAnimationDict.ContainsKey(newItem))
            {
                currentAttackAnimSet = weaponAnimationDict[newItem];
            }
        }
        else if (newItem == null && oldItem != null && oldItem.equipSlot == EquipmentSlot.Weapon)
        {
            currentAttackAnimSet = defaultAttackAnimSet;
        }
    }

    [System.Serializable]
    public struct WeaponAnimations
    {
        public Equipment weapon;
        public AnimationClip[] clips;
    }
}
