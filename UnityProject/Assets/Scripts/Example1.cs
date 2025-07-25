namespace UnityProject
{
    /// <summary>
    /// Every class and member should have a one sentence
    /// summary describing its purpose.
    /// </summary>
    /// <remarks>
    /// You can expand on that one sentence summary to
    /// provide more information for readers. In this case,
    /// the <c>ExampleClass</c> provides different C#
    /// elements to show how you would add documentation
    ///comments for most elements in a typical class.
    /// <para>
    /// The remarks can add multiple paragraphs, so you can
    /// write detailed information for developers that use
    /// your work. You should add everything needed for
    /// readers to be successful. This class contains
    /// examples for the following:
    /// </para>
    /// <list type="table">
    /// <item>
    /// <term>Summary</term>
    /// <description>
    /// This should provide a one sentence summary of the class or member.
    /// </description>
    /// </item>
    /// <item>
    /// <term>Remarks</term>
    /// <description>
    /// This is typically a more detailed description of the class or member
    /// </description>
    /// </item>
    /// <item>
    /// <term>para</term>
    /// <description>
    /// The para tag separates a section into multiple paragraphs
    /// </description>
    /// </item>
    /// <item>
    /// <term>list</term>
    /// <description>
    /// Provides a list of terms or elements
    /// </description>
    /// </item>
    /// <item>
    /// <term>returns, param</term>
    /// <description>
    /// Used to describe parameters and return values
    /// </description>
    /// </item>
    /// <item>
    /// <term>value</term>
    /// <description>Used to describe properties</description>
    /// </item>
    /// <item>
    /// <term>exception</term>
    /// <description>
    /// Used to describe exceptions that may be thrown
    /// </description>
    /// </item>
    /// <item>
    /// <term>c, cref, see, seealso</term>
    /// <description>
    /// These provide code style and links to other
    /// documentation elements
    /// </description>
    /// </item>
    /// <item>
    /// <term>example, code</term>
    /// <description>
    /// These are used for code examples
    /// </description>
    /// </item>
    /// </list>
    /// <para>
    /// The list above uses the "table" style. You could
    /// also use the "bullet" or "number" style. Neither
    /// would typically use the "term" element.
    /// <br/>
    /// Note: paragraphs are double spaced. Use the *br*
    /// tag for single spaced lines.
    /// </para>
    /// </remarks>
    public class ExampleClass
    {
        /// <value>
        /// The <c>Label</c> property represents a label
        /// for this instance.
        /// </value>
        /// <remarks>
        /// The <see cref="Label"/> is a <see langword="string"/>
        /// that you use for a label.
        /// <para>
        /// Note that there isn't a way to provide a "cref" to
        /// each accessor, only to the property itself.
        /// </para>
        /// </remarks>
        public string? Label
        {
            get;
            set;
        }

        /// <summary>
        /// Adds two integers and returns the result.
        /// </summary>
        /// <returns>
        /// The sum of two integers.
        /// </returns>
        /// <param name="left">
        /// The left operand of the addition.
        /// </param>
        /// <param name="right">
        /// The right operand of the addition.
        /// </param>
        /// <example>
        /// <code>
        /// int c = Math.Add(4, 5);
        /// if (c > 10)
        /// {
        ///     Console.WriteLine(c);
        /// }
        /// </code>
        /// </example>
        /// <exception cref="System.OverflowException">
        /// Thrown when one parameter is
        /// <see cref="Int32.MaxValue">MaxValue</see> and the other is
        /// greater than 0.
        /// Note that here you can also use
        /// <see href="https://learn.microsoft.com/dotnet/api/system.int32.maxvalue"/>
        ///  to point a web page instead.
        /// </exception>
        /// <see cref="ExampleClass"/> for a list of all
        /// the tags in these examples.
        /// <seealso cref="ExampleClass.Label"/>
        public static int Add(int left, int right)
        {
            if ((left == int.MaxValue && right > 0) || (right == int.MaxValue && left > 0))
                throw new System.OverflowException();

            return left + right;
        }
    }
}
