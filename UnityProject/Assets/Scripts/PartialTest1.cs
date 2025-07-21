using System;
using UnityEngine.UI;

namespace UnityProject
{
    /// <summary>
    /// A partial test class to verify partial class merging in documentation extraction.
    /// This is the first part of the partial class definition.
    /// </summary>
    public partial class PartialTestClass
    {
        /// <summary>
        /// A public field from the first partial file.
        /// </summary>
        public int FirstPartField = 10;

        /// <summary>
        /// A private field that should not appear in package docs.
        /// </summary>
        private string firstPrivateField = "first";

        /// <summary>
        /// A method from the first partial class file.
        /// </summary>
        /// <param name="value">Input value to process</param>
        /// <returns>Processed string value</returns>
        public string ProcessFromFirstPart(int value)
        {
            return $"First part processed: {value}";
        }

        /// <summary>
        /// A private method from the first partial file.
        /// </summary>
        /// <param name="data">Data to process privately</param>
        /// <returns>Processed data</returns>
        private string ProcessPrivatelyFromFirst(string data)
        {
            return data.ToLower();
        }
    }
}