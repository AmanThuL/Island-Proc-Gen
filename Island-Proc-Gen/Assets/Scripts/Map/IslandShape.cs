using System.Collections;
using System.Collections.Generic;
using System.Runtime.CompilerServices;
using UnityEngine;
using Random = UnityEngine.Random;

namespace Assets.Map
{
    /// <summary>
    /// This class has factory functions for generating islands of
    /// different shapes.
    /// </summary>
    public class IslandShape
    {
        // The radial island radius is based on overlapping sine waves 
        public static float ISLAND_FACTOR = 1.07f;  // 1.0 means no small islands; 2.0 leads to a lot

        private static NoiseSettings settings;

        public static int mapWidth;
        public static int mapHeight;

        // Randomness
        private static System.Random prng;
        public static Vector2 offset;

        //// Noise variables
        //private static Vector2[] octaveOffsets;

        public static void SetupIslandShape(int _width, int _height, NoiseSettings _settings)
        {
            // Create island shape
            mapWidth = _width;
            mapHeight = _height;

            // Initialize PRNG
            _settings.seed = _settings.randomizedSeed ? Random.Range(int.MinValue, int.MaxValue) : _settings.seed;
            MapStats.Instance.seed = _settings.seed;
            prng = new System.Random(_settings.seed);

            //// Initialize octave offsets array
            //octaveOffsets = new Vector2[_settings.octaves];
            
            settings = _settings;
        }

        #region Perlin
        // The Perlin-based island combines perlin noise with the radius
        public static System.Func<Vector2, bool> makePerlin()
        {
            offset = new Vector2(prng.Next(-100000, 100000) + settings.offset.x,
                                         prng.Next(-100000, 100000) - settings.offset.y);
           
            float halfWidth = mapWidth / 2f;
            float halfHeight = mapHeight / 2f;

            float landRatioMinimum = 0.1f;
            float landRatioMaximum = 0.5f;
            float OCEAN_RATIO = ((landRatioMaximum - landRatioMinimum) * settings.oceanRatio) + landRatioMinimum;

            System.Func<Vector2, bool> inside = q =>
            {
                q = new Vector2(q.x / halfWidth - 1, q.y / halfHeight - 1);
                float x = (q.x + offset.x) * settings.scale;
                float y = (q.y + offset.y) * settings.scale;
                float perlin = Mathf.PerlinNoise(x, y);
                return perlin > OCEAN_RATIO + OCEAN_RATIO * Mathf.Pow(q.magnitude, settings.exponent);
            };
            return inside;
        }
        #endregion


        #region Radial
        public static System.Func<Vector2, bool> makeRadial()
        {
            var bumps = Random.Range(1, 6);
            var startAngle = Random.value * 2 * Mathf.PI;
            var dipAngle = Random.value * 2 * Mathf.PI;

            var random = Random.value;
            var start = 0.2f;
            var end = 0.7f;

            var dipWidth = (end - start) * random + start;

            System.Func<Vector2, bool> inside = q =>
            {
                var angle = Mathf.Atan2(q.y, q.x);
                var length = 0.5 * (Mathf.Max(Mathf.Abs(q.x), Mathf.Abs(q.y)) + q.magnitude);

                var r1 = 0.5 + 0.40 * Mathf.Sin(startAngle + bumps * angle + Mathf.Cos((bumps + 3) * angle));
                var r2 = 0.7 - 0.20 * Mathf.Sin(startAngle + bumps * angle - Mathf.Sin((bumps + 2) * angle));
                if (Mathf.Abs(angle - dipAngle) < dipWidth
                    || Mathf.Abs(angle - dipAngle + 2 * Mathf.PI) < dipWidth
                    || Mathf.Abs(angle - dipAngle - 2 * Mathf.PI) < dipWidth)
                {
                    r1 = r2 = 0.2;
                }
                var result = (length < r1 || (length > r1 * ISLAND_FACTOR && length < r2));
                return result;
            };

            return inside;
        }
        #endregion


        #region Square
        // The square shape fills the entire space with land
        public static System.Func<Vector2, bool> makeSquare()
        {
            System.Func<Vector2, bool> inside = q => { return true; };
            return inside;
        }
        #endregion
    }
}
