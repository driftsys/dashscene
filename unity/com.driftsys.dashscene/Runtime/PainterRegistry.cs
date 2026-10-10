// The painters the ordered draw pass draws, and when it subscribes to the
// camera callback (story #1448).
//
// **Unity-free on purpose, like `DrawRouting` beside it.** `unity/ffi-check`
// executes it. `OrderedDrawPass` keeps only the engine calls: the subscription
// to `RenderPipelineManager.beginCameraRendering`, which the answers of `Add`
// and `Remove` below decide.

using System.Collections.Generic;

namespace Driftsys.Dashscene
{
    /// An ordered set of registered painters, in registration order.
    ///
    /// **Each answer says when the caller's subscription changes.** The pass
    /// subscribes to the camera callback once, when the first painter
    /// registers, and unsubscribes once, when the last one leaves — so a
    /// process with no painter pays nothing on any camera, and a second
    /// subscription would draw every run twice.
    internal sealed class PainterRegistry<T>
        where T : class
    {
        private readonly List<T> _items = new List<T>();

        /// How many painters are registered.
        internal int Count => _items.Count;

        /// The painter registered `index`-th, counting from zero.
        internal T this[int index] => _items[index];

        /// Register `item` once.
        ///
        /// Returns `true` exactly when this call took the set from empty to
        /// one painter, which is when the caller subscribes. A `null` or an
        /// already registered painter changes nothing and returns `false`.
        internal bool Add(T item)
        {
            if (item == null || _items.Contains(item))
            {
                return false;
            }

            _items.Add(item);
            return _items.Count == 1;
        }

        /// Remove `item`.
        ///
        /// Returns `true` exactly when this call took the set from one
        /// painter to empty, which is when the caller unsubscribes. A painter
        /// that is not registered changes nothing and returns `false`.
        internal bool Remove(T item)
        {
            if (!_items.Remove(item))
            {
                return false;
            }

            return _items.Count == 0;
        }

        /// Forget every painter, without answering whether to unsubscribe.
        ///
        /// For a reset that unsubscribes unconditionally.
        internal void Clear()
        {
            _items.Clear();
        }
    }
}
