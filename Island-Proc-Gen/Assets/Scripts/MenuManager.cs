using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using TMPro;
using UnityEngine.UI;
using UnityEngine.EventSystems;
using System.Reflection;
using UnityEngine.SceneManagement;

public class MenuManager : MonoBehaviour
{
    public Text seedText;

    private PointerEventData pointer;

    IEnumerator Start()
    {
        pointer = new PointerEventData(EventSystem.current);

        yield return new WaitForSeconds(1);
        seedText.text = "" + MapStats.Instance.seed.ToString();
    }

    private void Update()
    {
        if (Input.GetKeyDown(KeyCode.Escape))
        {
            Application.Quit();
        }
    }

    public void Play()
    {
        if (seedText.text != "")
        {
            Debug.Log(seedText.text);
            int.TryParse(seedText.text, out int inputSeed);
            MapStats.Instance.seed = inputSeed;
        }
        SceneManager.LoadScene("MainIsland");
    }
}
