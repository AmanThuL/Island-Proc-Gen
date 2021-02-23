using System.Collections;
using System.Collections.Generic;
using UnityEngine;

[RequireComponent(typeof(CharacterStats))]
public class CharacterCombat : MonoBehaviour
{
	[SerializeField]
	private float attackSpeed = 1f;

	private float attackCooldown = 0f;
	const float combatCooldown = 5;
	float lastAttackTime;

	[SerializeField]
	private float attackDelay = .6f;

	public bool InCombat { get; private set; }
	public event System.Action OnAttack;

	CharacterStats myStats;
	CharacterStats opponentStats;

	void Start()
	{
		myStats = GetComponent<CharacterStats>();
	}

	void Update()
	{
		attackCooldown -= Time.deltaTime;

		if (Time.time - lastAttackTime > combatCooldown)
		{
			InCombat = false;
		}
	}

	public void Attack(CharacterStats targetStats)
	{
		if (attackCooldown <= 0f)
		{
			opponentStats = targetStats;
            OnAttack?.Invoke();

            attackCooldown = 1f / attackSpeed;
			InCombat = true;
			lastAttackTime = Time.time;
		}

	}

	public void AttackHit_AnimationEvent()
	{
		if (opponentStats != null)
        {
			opponentStats.TakeDamage(myStats.damage.GetValue());
		}
		if (opponentStats.CurrentHealth <= 0)
		{
			InCombat = false;
		}
	}

}
