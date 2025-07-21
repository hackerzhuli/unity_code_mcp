namespace UnityProject
{
    public class Inherit1
    {
        /// <inheritdoc cref="Add(int, int, int)"/>
        public void Add() {}

        /// <inheritdoc cref="Add{T}(T, int, int)"/>
        public void Add2(){    }

        /// <inheritdoc cref="Add(ref int, out int, in System.Int32)"/>
        public void Add3(){}

        /// <inheritdoc cref="Add5"/> /* parameters can be ommited if there is just one overload of the same name */
        public void Add4(int a, int b){        }

        /// <summary>
        /// doc for add 5
        /// </summary>
        /// <remarks>remarks from Add5</remarks>
        /// <returns>return from Add5</returns>
        public void Add5(){}

        ///<summary>
        /// <inheritdoc cref="Add5"/>
        /// </summary>
        /// <remarks>remarks from Add6</remarks>
        public void Add6(){}    // should only inherit summary content from Add5, and the rest is kept as is

        /// <inheritdoc cref="Add5"/>
        /// <remarks>remarks from Add7</remarks>
        public void Add7(){ } // inherit everything from Add5, but override remarks content with remarks from Add7

        /// <summary>
        /// doc for generic add
        /// </summary>
        /// <param name="a"></param>
        /// <param name="b"></param>
        /// <typeparam name="T"></typeparam>
        /// <returns></returns>
        public T Add<T>(T a, T b)
        {
            return a;
        }

        /// <summary>
        /// doc for generic add
        /// </summary>
        /// <param name="a"></param>
        /// <param name="b"></param>
        /// <param name="c"></param>
        /// <typeparam name="T"></typeparam>
        /// <returns></returns>
        public T Add<T>(T a, int b, int c)
        {
            return a;
        }

        /// <summary>
        /// doc for add with 3 parameters
        /// </summary>
        /// <param name="a"></param>
        /// <param name="b"></param>
        /// <param name="c"></param>
        /// <returns></returns>
        public int Add(int a, int b, int c)
        {
            return a + b;
        }

        /// <summary>
        /// doc for add with 3 parameters complex
        /// </summary>
        /// <param name="a"></param>
        /// <param name="b"></param>
        /// <param name="c"></param>
        /// <returns></returns>
        public int Add(ref int a, out int b, in int c)
        {
            b = a + c;
            return b;
        }
    }

    public class Inherit2
    {
        
    }
}