
namespace UnityProject
{
    /// <summary>
    /// A simple test class for documentation extraction.
    /// Contains various member types for testing.
    /// </summary>
    public class TestClass
    {
        /// <summary>
        /// A public field for testing field documentation.
        /// </summary>
        public int PublicField = 42;

        /// <summary>
        /// A private field that should not appear in package docs.
        /// </summary>
        private string privateField = "hidden";

        /// <summary>
        /// Adds two numbers together.
        /// </summary>
        /// <param name="a">First number</param>
        /// <param name="b">Second number</param>
        /// <returns>The sum of a and b</returns>
        public int Add(int a, int b)
        {
            return a + b;
        }

        /// <summary>
        /// A private method that should not appear in package docs.
        /// </summary>
        /// <param name="value">Input value</param>
        /// <returns>Processed value</returns>
        private string ProcessPrivately(string value)
        {
            return value.ToUpper();
        }

        /// <summary>
        /// Gets or sets the test property.
        /// </summary>
        /// <value>The test value</value>
        public string TestProperty { get; set; } = "default";

        // Method without XML documentation - should be excluded
        public void UndocumentedMethod()
        {
            Console.WriteLine("This method has no XML docs");
        }

        // Another undocumented method with parameters
        public string UndocumentedMethodWithParams(int value, string text)
        {
            return $"{text}: {value}";
        }

        // Undocumented property
        public bool UndocumentedProperty { get; set; }

        // Undocumented field
        public static readonly string UndocumentedField = "no docs";

        // Private undocumented method (should be excluded anyway)
        private void UndocumentedPrivateMethod()
        {
            // No implementation
        }

        /// <summary>
        /// A nested public class for testing nested type extraction.
        /// </summary>
        public class NestedPublicClass
        {
            /// <summary>
            /// A method inside the nested class.
            /// </summary>
            /// <returns>Always returns true</returns>
            public bool IsNested()
            {
                return true;
            }
        }
    }

    /// <summary>
    /// A private class with public methods.
    /// This should not appear in package documentation.
    /// </summary>
    class PrivateClass
    {
        /// <summary>
        /// A public method in a private class.
        /// </summary>
        /// <returns>Test string</returns>
        public string GetTestString()
        {
            return 1;
        }
 
        /// <summary>
        /// Another public method in the private class.
        /// </summary>
        /// <param name="count">Number of iterations</param>
        public void DoSomething(int count)
        {
            // Implementation here
            Button button;
        }
    }
}