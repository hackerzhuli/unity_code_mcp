using System;
using UnityEngine.EventSystems;

namespace UnityProject
{
    /// <summary>
    /// This is the second part of the partial class definition.
    /// Contains additional members for the PartialTestClass.
    /// </summary>
    public partial class PartialTestClass
    {
        /// <summary>
        /// A public field from the second partial file.
        /// </summary>
        public string SecondPartField = "second";

        /// <summary>
        /// A private field from the second partial file.
        /// </summary>
        private bool secondPrivateField = true;

        /// <summary>
        /// A property from the second partial class file.
        /// </summary>
        /// <value>The combined value from both parts</value>
        public string CombinedProperty { get; set; } = "combined";

        /// <summary>
        /// A method from the second partial class file.
        /// </summary>
        /// <param name="input">Input string to transform</param>
        /// <returns>Transformed string</returns>
        public string ProcessFromSecondPart(string input)
        {
            return $"Second part processed: {input}";
        }

        /// <summary>
        /// Another public method that combines data from both parts.
        /// </summary>
        /// <returns>Combined result from both partial class parts</returns>
        public string CombineFromBothParts()
        {
            return $"{FirstPartField} - {SecondPartField} - {CombinedProperty}";
        }

        /// <summary>
        /// A private method from the second partial file.
        /// </summary>
        /// <param name="count">Count value</param>
        /// <returns>Processed count</returns>
        private int ProcessPrivatelyFromSecond(int count)
        {
            return count * 2;
        }
    }
}