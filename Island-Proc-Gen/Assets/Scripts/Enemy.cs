using System.Collections;
using System.Collections.Generic;
using UnityEngine;

/// <summary>
/// Handles interaction with the Enemy.
/// </summary>
/// 
[RequireComponent(typeof(CharacterStats))]
public class Enemy : Interactable
{
    private PlayerManager playerManager;
    private CharacterStats myStats;

    void Start()
    {
        playerManager = PlayerManager.Instance;
        myStats = GetComponent<CharacterStats>();
    }

    // When we interact with the enemy: We attack it.
    public override void Interact()
    {
        base.Interact();
        CharacterCombat playerCombat = playerManager.player.GetComponent<CharacterCombat>();
        if (playerCombat != null)
        {
            playerCombat.Attack(myStats);
        }
    }
}
