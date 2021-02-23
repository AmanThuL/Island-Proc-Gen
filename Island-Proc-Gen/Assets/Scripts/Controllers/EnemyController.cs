using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.AI;

/// <summary>
/// Controls the Enemy AI
/// </summary>
public class EnemyController : MonoBehaviour
{
    [SerializeField]
    private float lookRadius = 10f;         // Detection range for player
    [SerializeField]
    private float faceTargetRotSpeed = 5f;

    private Transform target;               // Reference to the player
    private NavMeshAgent agent;             // Reference to the NavMeshAgent
    private CharacterCombat combat;

    // Start is called before the first frame update
    void Start()
    {
        target = PlayerManager.Instance.player.transform;
        agent = GetComponent<NavMeshAgent>();
        combat = GetComponent<CharacterCombat>();
    }

    // Update is called once per frame
    void Update()
    {
        float distance = Vector3.Distance(target.position, transform.position);

        if (distance <= lookRadius)
        {
            agent.SetDestination(target.position);

            if (distance <= agent.stoppingDistance)
            {
                CharacterStats targetStats = target.GetComponent<CharacterStats>();
                if (targetStats != null)
                {
                    Debug.Log("Attack player!!");
                    combat.Attack(targetStats);
                }

                FaceTarget();   // Make sure to face towards the target
            }
        }
    }

    private void FaceTarget()
    {
        Vector3 direction = (target.position - transform.position).normalized;
        Quaternion lookRotation = Quaternion.LookRotation(new Vector3(direction.x, 0, direction.z));
        transform.rotation = Quaternion.Slerp(transform.rotation, lookRotation, Time.deltaTime * faceTargetRotSpeed);
    }

    private void OnDrawGizmosSelected()
    {
        Gizmos.color = Color.red;
        Gizmos.DrawWireSphere(transform.position, lookRadius);
    }
}
