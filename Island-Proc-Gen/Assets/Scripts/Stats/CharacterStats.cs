using UnityEngine;

public class CharacterStats : MonoBehaviour
{
    public int maxHealth = 100;
    public int CurrentHealth { get; private set; }

    public Stat damage;
    public Stat armor;

    public event System.Action<int, int> OnHealthChanged;

    public virtual void Start()
    {
        CurrentHealth = maxHealth;
    }

    void Update()
    {
        if (Input.GetKeyDown(KeyCode.T))
        {
            TakeDamage(10);
        }
    }

    public void TakeDamage(int damage)
    {
        // Subtract the armor value
        damage -= armor.GetValue();
        damage = Mathf.Clamp(damage, 0, int.MaxValue);

        // Damage the character
        CurrentHealth -= damage;

        if (OnHealthChanged != null)
        {
            OnHealthChanged(maxHealth, CurrentHealth);
        }

        // If health reaches zero
        if (CurrentHealth <= 0)
        {
            Die();
        }
    }

    public virtual void Die()
    {
        // Die in some way
        // This method is meant to be overwritten
        Debug.Log(transform.name + " died.");
    }
}
