// Holds the C# declarations in `unity/com.driftsys.dashscene/Runtime/BoundaryB.cs`
// to the layouts `crates/dashpaint-abi` reports for the same types.
//
// It compiles that file rather than a copy of it, so what runs here is what the
// UPM package ships. It needs no Unity editor: a plain .NET toolchain and a
// cdylib build of the gate crate are the whole prerequisite.
//
// WHAT THIS PROVES, MEMBER BY MEMBER
//
// The Rust side reports, for every type on the surface, its name and each
// member's name, offset and size. This compares all of it against the C#
// declaration, matching members BY NAME rather than by position:
//
//   * Every type the macro declares has a C# declaration, and every C#
//     declaration is one the macro declares. Both directions, so a type added
//     on one side alone fails.
//   * Total size agrees, per type.
//   * Every member's offset agrees, so two members exchanged are caught even
//     when both are four bytes wide and the totals are identical.
//   * Every member's size agrees, so a widened member is caught even when the
//     padding that followed it absorbs the growth and no offset moves. An
//     enum declared without `: byte` — C#'s default is `int` — is exactly that
//     case, and it is the easiest mistake to make in this file.
//   * The call path works: a value crosses by value and returns unchanged.
//
// Alignment carries no assertion of its own and needs none: alignment is what
// decides padding, padding is what decides the offsets, and every offset is
// compared above. A `[StructLayout(Pack = 1)]` on a C# declaration moves an
// offset and fails there.
//
// WHAT IT STILL CANNOT SEE — two things, both measured
//
// A member whose C# type has the same size but different meaning — `uint`
// declared as `float`. Both are four bytes at the same offset, and the
// round-trip returns the same bytes into the same member, so nothing here
// separates them.
//
// A member added to the RUST type that fits inside padding already there.
// `abi_surface!`'s member lists are hand-written, so an unlisted member is
// reported by nothing, and one that fits in padding moves no size and no
// offset either. Adding `quality: u8` to `dashpaint::Blur` between `kind` and
// `radius` leaves this check green while `BoundaryB.cs` is missing a member.
// Issue #1252 closed it, and not the way this comment predicted: a struct
// expression inside the existing `macro_rules!` is exhaustive on stable, so
// `abi_surface!` rebuilds each type from its declared members and an unlisted
// member fails the build with `E0063`. No proc-macro crate and no new
// dependency for `dashpaint`. What that comment expected instead was a derive,
// and `dashpaint` declares no dependencies at all today.
//
// Reading `BoundaryB.cs` against `crates/dashpaint/src/lib.rs` is what catches
// both.

using System.Globalization;
using System.Reflection;
using System.Runtime.InteropServices;
using Driftsys.Dashscene.AbiCheck;
using Driftsys.Dashscene.BoundaryB;

var libPath = Environment.GetEnvironmentVariable("DASHPAINT_ABI_LIB");
if (string.IsNullOrWhiteSpace(libPath) || !File.Exists(libPath))
{
    // Refuse rather than fall back to the loader path: a check that silently
    // loads some other build of this library reports on the wrong artifact.
    Console.Error.WriteLine("abi-check: set DASHPAINT_ABI_LIB to the cdylib to check against.");
    Console.Error.WriteLine("abi-check: `just unity-abi` builds it and sets it.");
    return 2;
}

NativeLibrary.SetDllImportResolver(
    typeof(Native).Assembly,
    (name, asm, path) => name == Native.Lib ? NativeLibrary.Load(libPath) : IntPtr.Zero);

Console.WriteLine($"abi-check: library {libPath}");

var failures = new List<string>();
var declared = Native.Subjects.ToDictionary(s => s.Name, StringComparer.Ordinal);
var matched = new HashSet<string>(StringComparer.Ordinal);

uint rustTypes;
try
{
    rustTypes = Native.dashpaint_abi_type_count();
}
catch (Exception e)
{
    Console.Error.WriteLine($"abi-check: could not call the library: {e.Message}");
    return 2;
}

if (rustTypes != declared.Count)
{
    failures.Add($"the macro declares {rustTypes} types and this check declares {declared.Count}");
}

for (uint i = 0; i < rustTypes; i++)
{
    var typeName = Marshal.PtrToStringAnsi(Native.dashpaint_abi_type_name(i));
    if (string.IsNullOrEmpty(typeName))
    {
        failures.Add($"type index {i}: the library reported no name");
        continue;
    }
    if (!declared.TryGetValue(typeName, out var subject))
    {
        failures.Add($"{typeName}: on the Rust surface, and not declared in BoundaryB.cs");
        continue;
    }
    matched.Add(typeName);

    try
    {
        var before = failures.Count;
        Check(subject, i, failures);
        if (failures.Count == before)
        {
            var layout = (AbiLayout)subject.Layout.Invoke(null, null)!;
            Console.WriteLine($"  ok  {subject.Name,-14} {layout.Size,3} bytes, "
                              + $"align {layout.Align}, "
                              + $"{Native.dashpaint_abi_field_count(i)} members");
        }
    }
    catch (Exception e)
    {
        var inner = e is TargetInvocationException t && t.InnerException is not null
            ? t.InnerException : e;
        failures.Add($"{typeName}: {inner.GetType().Name}: {inner.Message}");
    }
}

// **Enumerated from the package's own file, not from the declarations above.**
// Comparing against `NativeMethods.cs` would only ask whether this check
// declares what it declares; the claim is about what the UPM package ships.
foreach (var t in typeof(Color).Assembly.GetTypes())
{
    if (t.Namespace != "Driftsys.Dashscene.BoundaryB") continue;
    if (!t.IsValueType || t.IsEnum) continue;
    // `Float4` and `UInt4` stand in for Rust's `[f32; 4]` and `[u32; 4]`, which
    // are members rather than types, so no Rust type carries their name. They
    // are named here rather than skipped by a rule, so a genuinely new
    // unchecked struct is refused instead of joining them silently.
    if (t.Name is "Float4" or "UInt4") continue;
    if (!matched.Contains(t.Name))
    {
        failures.Add($"{t.Name}: declared in BoundaryB.cs, and not on the Rust surface");
    }
}

if (failures.Count > 0)
{
    Console.Error.WriteLine($"\nabi-check: {failures.Count} failure(s):");
    foreach (var f in failures) Console.Error.WriteLine($"  - {f}");
    return 1;
}

Console.WriteLine($"\nabi-check: {rustTypes} types agree with crates/dashpaint-abi, "
                  + "member by member.");
return 0;

void Check(Subject subject, uint typeIndex, List<string> into)
{
    var layout = (AbiLayout)subject.Layout.Invoke(null, null)!;
    var size = Marshal.SizeOf(subject.Type);
    if (size != layout.Size)
    {
        into.Add($"{subject.Name}: Rust reports {layout.Size} bytes, the C# declaration is {size}");
        return;
    }

    var csFields = subject.Type.GetFields(BindingFlags.Public | BindingFlags.Instance);
    var rustFieldCount = Native.dashpaint_abi_field_count(typeIndex);
    if (rustFieldCount != csFields.Length)
    {
        into.Add($"{subject.Name}: Rust reports {rustFieldCount} members, "
                 + $"the C# declaration has {csFields.Length}");
        return;
    }

    for (uint j = 0; j < rustFieldCount; j++)
    {
        var field = Native.dashpaint_abi_field(typeIndex, j);
        var rustName = Marshal.PtrToStringAnsi(field.Name);
        if (string.IsNullOrEmpty(rustName))
        {
            into.Add($"{subject.Name}: the library reported no name for member {j}");
            continue;
        }

        var expected = Pascalise(rustName);
        var cs = Array.Find(csFields, f => f.Name == expected);
        if (cs is null)
        {
            into.Add($"{subject.Name}.{rustName}: no C# member named {expected}");
            continue;
        }

        var offset = (uint)Marshal.OffsetOf(subject.Type, expected);
        if (offset != field.Offset)
        {
            into.Add($"{subject.Name}.{expected}: Rust puts it at offset {field.Offset}, "
                     + $"the C# declaration at {offset}");
        }

        var csSize = (uint)Marshal.SizeOf(
            cs.FieldType.IsEnum ? Enum.GetUnderlyingType(cs.FieldType) : cs.FieldType);
        if (csSize != field.Size)
        {
            into.Add($"{subject.Name}.{expected}: Rust reports size {field.Size}, "
                     + $"the C# declaration size {csSize}"
                     + (cs.FieldType.IsEnum ? " — is the enum missing `: byte`?" : ""));
        }
    }

    var seed = 1;
    var sent = Fill(subject.Type, ref seed);
    var back = subject.RoundTrip.Invoke(null, new[] { sent })!;
    var a = Flatten(sent);
    var b = Flatten(back);
    if (!a.SequenceEqual(b))
    {
        into.Add($"{subject.Name}: the round-trip changed the value — "
                 + $"sent [{string.Join("; ", a)}], received [{string.Join("; ", b)}]");
        return;
    }

}

/// `rotation_anchor` to `RotationAnchor`. The mapping is uniform across the
/// whole surface, so a C# member that does not follow it is reported as
/// missing rather than quietly skipped.
static string Pascalise(string snake) =>
    string.Concat(snake.Split('_', StringSplitOptions.RemoveEmptyEntries)
        .Select(part => char.ToUpperInvariant(part[0]) + part[1..]));

/// Builds a value whose every leaf member is a different number, so a
/// round-trip that dropped or zeroed one shows up as a changed leaf.
///
/// **Refuses an unhandled primitive rather than defaulting it.** Falling
/// through to `Activator.CreateInstance` would return a boxed zero with no
/// fields to recurse into, and the comparison would then be 0 against 0 — a
/// member silently exempted from the one check that covers it.
static object Fill(Type t, ref int seed)
{
    if (t.IsEnum)
    {
        var values = Enum.GetValues(t);
        return values.GetValue(seed++ % values.Length)!;
    }
    if (t == typeof(float)) return (float)(seed++) + 0.5f;
    if (t == typeof(uint)) return (uint)(seed++);
    if (t.IsPrimitive)
    {
        throw new NotSupportedException(
            $"Fill has no case for {t.FullName}; add one rather than letting it seed to zero");
    }

    var value = Activator.CreateInstance(t)!;
    foreach (var f in t.GetFields(BindingFlags.Public | BindingFlags.Instance))
    {
        f.SetValue(value, Fill(f.FieldType, ref seed));
    }
    return value;
}

/// The leaf members of a value, each labelled with its path.
///
/// Labelled rather than positional because `GetFields` does not promise
/// declaration order, and because the list is read by a person diagnosing a
/// red gate. Formatted with the invariant culture: on a comma-decimal locale
/// `2.5` renders as `2,5`, which is unreadable inside a joined list.
static List<string> Flatten(object value, string path = "")
{
    var t = value.GetType();
    if (t.IsEnum || t.IsPrimitive)
    {
        var text = value is IFormattable f
            ? f.ToString(null, CultureInfo.InvariantCulture)
            : value.ToString()!;
        return new List<string> { $"{path}={text}" };
    }

    var leaves = new List<string>();
    foreach (var field in t.GetFields(BindingFlags.Public | BindingFlags.Instance))
    {
        var child = path.Length == 0 ? field.Name : $"{path}.{field.Name}";
        leaves.AddRange(Flatten(field.GetValue(value)!, child));
    }
    return leaves;
}
