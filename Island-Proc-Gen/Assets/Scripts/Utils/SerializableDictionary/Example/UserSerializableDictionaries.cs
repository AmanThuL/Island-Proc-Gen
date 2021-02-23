using System;
using UnityEngine;

[Serializable]
public class BiomeGameObjectDictionary : SerializableDictionary<Biome, GameObject> {}

[Serializable]
public class BuildingTypeGameObjectDictionary : SerializableDictionary<BuildingType, GameObject> { }

[Serializable]
public class CursorTypeTextureDictionary : SerializableDictionary<CursorType, Texture2D> { }

[Serializable]
public class StringStringDictionary : SerializableDictionary<string, string> { }

[Serializable]
public class ObjectColorDictionary : SerializableDictionary<UnityEngine.Object, Color> {}

[Serializable]
public class ColorArrayStorage : SerializableDictionary.Storage<Color[]> {}

[Serializable]
public class StringColorArrayDictionary : SerializableDictionary<string, Color[], ColorArrayStorage> {}

[Serializable]
public class MyClass
{
    public int i;
    public string str;
}

[Serializable]
public class QuaternionMyClassDictionary : SerializableDictionary<Quaternion, MyClass> {}