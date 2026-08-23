// A JSON reader for `conformance/layer2-probes.json`, and nothing else.
//
// **Why a parser is written here at all.** Unity's `JsonUtility` maps JSON onto
// serializable fields and cannot represent this file: a probe's `args` is
// positional and heterogeneous — a number, an array of two, an array of eight
// arrays of four — and `expected` is a number or an array of four depending on
// the case. `com.unity.nuget.newtonsoft-json` is not one of the editor's
// built-in packages, so naming it in the throwaway project's manifest would
// send the resolve to the network for a gate that otherwise needs none.
//
// **The float parse is the part that matters**, and it is why this file exists
// rather than a hand-rolled `float.Parse` scattered through the harness.
// `conformance/README.md` asks a consumer for a correctly-rounded parser and
// gives the measurement behind the request. `DashsceneHlslConformance` runs
// [`SelfCheck`] before it reads the file, so a runtime whose `double.Parse` is
// not correctly rounded fails here and says so, rather than showing up later as
// a probe that is off in its last bits.
//
// Not part of the package: this file is copied into a throwaway Unity project
// by `just unity-conformance` and lives outside `unity/com.driftsys.dashscene/`.

using System;
using System.Collections.Generic;
using System.Globalization;
using System.Text;

/// <summary>What a <see cref="JsonValue"/> holds.</summary>
public enum JsonKind
{
    /// <summary>A number, in <see cref="JsonValue.Number"/>.</summary>
    Number,

    /// <summary>A string, in <see cref="JsonValue.Text"/>.</summary>
    Text,

    /// <summary>`true` or `false`, in <see cref="JsonValue.Bool"/>.</summary>
    Bool,

    /// <summary>`null`.</summary>
    Null,

    /// <summary>An array, in <see cref="JsonValue.Items"/>.</summary>
    Array,

    /// <summary>An object, in <see cref="JsonValue.Members"/>.</summary>
    Object,
}

/// <summary>One parsed JSON value.</summary>
public sealed class JsonValue
{
    /// <summary>Which of the fields below carries this value.</summary>
    public JsonKind Kind;

    /// <summary>The value of a <see cref="JsonKind.Number"/>.</summary>
    public double Number;

    /// <summary>The value of a <see cref="JsonKind.Text"/>.</summary>
    public string Text;

    /// <summary>The value of a <see cref="JsonKind.Bool"/>.</summary>
    public bool Bool;

    /// <summary>The entries of a <see cref="JsonKind.Array"/>.</summary>
    public List<JsonValue> Items;

    /// <summary>The members of a <see cref="JsonKind.Object"/>.</summary>
    public Dictionary<string, JsonValue> Members;

    /// <summary>The member named <paramref name="key"/>, or a thrown error.</summary>
    public JsonValue Member(string key)
    {
        if (Kind != JsonKind.Object)
        {
            throw new FormatException($"expected an object to read '{key}' from, found {Kind}");
        }

        if (!Members.TryGetValue(key, out var value))
        {
            throw new FormatException($"the object carries no '{key}'");
        }

        return value;
    }

    /// <summary>This value as a number, or a thrown error.</summary>
    public double AsNumber()
    {
        if (Kind != JsonKind.Number)
        {
            throw new FormatException($"expected a number, found {Kind}");
        }

        return Number;
    }

    /// <summary>This value as a string, or a thrown error.</summary>
    public string AsText()
    {
        if (Kind != JsonKind.Text)
        {
            throw new FormatException($"expected a string, found {Kind}");
        }

        return Text;
    }

    /// <summary>This value as an array, or a thrown error.</summary>
    public List<JsonValue> AsArray()
    {
        if (Kind != JsonKind.Array)
        {
            throw new FormatException($"expected an array, found {Kind}");
        }

        return Items;
    }

    /// <summary>
    /// This value as an array of exactly <paramref name="count"/> entries.
    /// </summary>
    public List<JsonValue> AsArray(int count)
    {
        var items = AsArray();
        if (items.Count != count)
        {
            throw new FormatException($"expected {count} entries, found {items.Count}");
        }

        return items;
    }
}

/// <summary>Parses the probe table, and checks its own float parse first.</summary>
public static class ProbeJson
{
    /// <summary>Parses one complete JSON document.</summary>
    /// <remarks>
    /// Refuses trailing content and duplicate object keys. A duplicate key is
    /// not a parse error in JSON and every parser resolves it silently one way
    /// or the other — which in a file read as committed truth is a field
    /// quietly replaced by another.
    /// </remarks>
    public static JsonValue Parse(string text)
    {
        var at = 0;
        var value = ParseValue(text, ref at);
        SkipWhitespace(text, ref at);
        if (at != text.Length)
        {
            throw new FormatException(
                $"trailing content at offset {at}: '{Near(text, at)}'");
        }

        return value;
    }

    /// <summary>
    /// The literals this reader must land exactly, and the doubles they are.
    /// </summary>
    /// <remarks>
    /// Each pair is a decimal string and the IEEE-754 bit pattern of the double
    /// nearest to it. They are the cases a parser that shortcuts — accumulating
    /// digits into a double and scaling by a power of ten — gets wrong:
    /// <c>1e23</c> lands one ULP high, <c>2.2250738585072011e-308</c> is the
    /// largest subnormal and the value that hung two production strtod
    /// implementations, and <c>7.8459735791271921e65</c> is a halfway case.
    /// <c>0.49000000953674316</c> is `conformance/README.md`'s own example, and
    /// <c>0.00392156862745098</c> is a tolerance the table actually carries.
    /// <para>
    /// The expected bit patterns were computed outside this harness. They are
    /// what "correctly rounded" means for these six inputs, so this check is
    /// not the parser marking its own work.
    /// </para>
    /// </remarks>
    private static readonly (string Literal, long Bits)[] FloatParseCases =
    {
        ("0.1", 0x3FB999999999999AL),
        ("0.49000000953674316", 0x3FDF5C2900000000L),
        ("1e23", 0x44B52D02C7E14AF6L),
        ("2.2250738585072011e-308", 0x000FFFFFFFFFFFFFL),
        ("7.8459735791271921e65", 0x4D9DCD0089C1314EL),
        ("0.00392156862745098", 0x3F70101010101010L),
    };

    /// <summary>
    /// Every problem with this reader's float parse, or an empty list.
    /// </summary>
    /// <remarks>
    /// Run before the table is read. `conformance/README.md` asks a consumer
    /// for a correctly-rounded parser; this is that request, checked on the
    /// runtime the harness is executing on rather than assumed from the
    /// framework's documentation.
    /// </remarks>
    public static List<string> SelfCheck()
    {
        var failures = new List<string>();
        foreach (var (literal, bits) in FloatParseCases)
        {
            var at = 0;
            double parsed;
            try
            {
                parsed = ParseValue(literal, ref at).AsNumber();
            }
            catch (Exception error)
            {
                failures.Add($"'{literal}' did not parse at all: {error.Message}");
                continue;
            }

            var got = BitConverter.DoubleToInt64Bits(parsed);
            if (got != bits)
            {
                failures.Add(
                    $"'{literal}' parsed to 0x{got:X16} and the correctly-rounded double is "
                    + $"0x{bits:X16}. This runtime's double.Parse is not correctly rounded, so "
                    + "every number this harness reads out of the probe table carries an error "
                    + "the table did not put there.");
            }
        }

        return failures;
    }

    private static JsonValue ParseValue(string text, ref int at)
    {
        SkipWhitespace(text, ref at);
        if (at >= text.Length)
        {
            throw new FormatException("the document ends where a value was expected");
        }

        switch (text[at])
        {
            case '{':
                return ParseObject(text, ref at);
            case '[':
                return ParseArray(text, ref at);
            case '"':
                return new JsonValue { Kind = JsonKind.Text, Text = ParseString(text, ref at) };
            case 't':
                Expect(text, ref at, "true");
                return new JsonValue { Kind = JsonKind.Bool, Bool = true };
            case 'f':
                Expect(text, ref at, "false");
                return new JsonValue { Kind = JsonKind.Bool, Bool = false };
            case 'n':
                Expect(text, ref at, "null");
                return new JsonValue { Kind = JsonKind.Null };
            default:
                return ParseNumber(text, ref at);
        }
    }

    private static JsonValue ParseObject(string text, ref int at)
    {
        at++;
        var members = new Dictionary<string, JsonValue>();
        SkipWhitespace(text, ref at);
        if (At(text, at) == '}')
        {
            at++;
            return new JsonValue { Kind = JsonKind.Object, Members = members };
        }

        while (true)
        {
            SkipWhitespace(text, ref at);
            var key = ParseString(text, ref at);
            SkipWhitespace(text, ref at);
            if (At(text, at) != ':')
            {
                throw new FormatException($"expected ':' at offset {at}: '{Near(text, at)}'");
            }

            at++;
            var value = ParseValue(text, ref at);
            if (members.ContainsKey(key))
            {
                throw new FormatException(
                    $"the object at offset {at} carries '{key}' twice. A duplicate key is legal "
                    + "JSON and resolves silently, which in a file read as committed truth is a "
                    + "field replaced by another without a word.");
            }

            members.Add(key, value);
            SkipWhitespace(text, ref at);
            var next = At(text, at);
            at++;
            if (next == '}')
            {
                return new JsonValue { Kind = JsonKind.Object, Members = members };
            }

            if (next != ',')
            {
                throw new FormatException(
                    $"expected ',' or '}}' at offset {at - 1}: '{Near(text, at - 1)}'");
            }
        }
    }

    private static JsonValue ParseArray(string text, ref int at)
    {
        at++;
        var items = new List<JsonValue>();
        SkipWhitespace(text, ref at);
        if (At(text, at) == ']')
        {
            at++;
            return new JsonValue { Kind = JsonKind.Array, Items = items };
        }

        while (true)
        {
            items.Add(ParseValue(text, ref at));
            SkipWhitespace(text, ref at);
            var next = At(text, at);
            at++;
            if (next == ']')
            {
                return new JsonValue { Kind = JsonKind.Array, Items = items };
            }

            if (next != ',')
            {
                throw new FormatException(
                    $"expected ',' or ']' at offset {at - 1}: '{Near(text, at - 1)}'");
            }
        }
    }

    private static string ParseString(string text, ref int at)
    {
        if (At(text, at) != '"')
        {
            throw new FormatException($"expected a string at offset {at}: '{Near(text, at)}'");
        }

        at++;
        var built = new StringBuilder();
        while (true)
        {
            if (at >= text.Length)
            {
                throw new FormatException("the document ends inside a string");
            }

            var c = text[at++];
            if (c == '"')
            {
                return built.ToString();
            }

            if (c != '\\')
            {
                built.Append(c);
                continue;
            }

            if (at >= text.Length)
            {
                throw new FormatException("the document ends inside an escape");
            }

            var escape = text[at++];
            switch (escape)
            {
                case '"': built.Append('"'); break;
                case '\\': built.Append('\\'); break;
                case '/': built.Append('/'); break;
                case 'b': built.Append('\b'); break;
                case 'f': built.Append('\f'); break;
                case 'n': built.Append('\n'); break;
                case 'r': built.Append('\r'); break;
                case 't': built.Append('\t'); break;
                case 'u':
                    if (at + 4 > text.Length)
                    {
                        throw new FormatException("the document ends inside a \\u escape");
                    }

                    built.Append(
                        (char)ushort.Parse(
                            text.Substring(at, 4),
                            NumberStyles.HexNumber,
                            CultureInfo.InvariantCulture));
                    at += 4;
                    break;
                default:
                    throw new FormatException($"unknown escape '\\{escape}' at offset {at - 1}");
            }
        }
    }

    /// <summary>
    /// One number token, delimited and then handed whole to
    /// <c>double.Parse</c>.
    /// </summary>
    /// <remarks>
    /// Handing the whole literal over is what makes the parse correctly
    /// rounded: the rounding is done once by an implementation that knows how,
    /// rather than accumulated digit by digit here.
    /// <see cref="SelfCheck"/> measures the result.
    /// <para>
    /// The scan finds the token's end; it is not a JSON validator, and it
    /// accepts a leading zero that the grammar forbids. This reads one
    /// committed file, and a malformed number reaches <c>double.Parse</c>,
    /// which refuses it.
    /// </para>
    /// </remarks>
    private static JsonValue ParseNumber(string text, ref int at)
    {
        var start = at;
        if (At(text, at) == '-')
        {
            at++;
        }

        var digits = 0;
        while (at < text.Length && text[at] >= '0' && text[at] <= '9')
        {
            at++;
            digits++;
        }

        if (digits == 0)
        {
            throw new FormatException($"expected a number at offset {start}: '{Near(text, start)}'");
        }

        if (at < text.Length && text[at] == '.')
        {
            at++;
            var fraction = 0;
            while (at < text.Length && text[at] >= '0' && text[at] <= '9')
            {
                at++;
                fraction++;
            }

            if (fraction == 0)
            {
                throw new FormatException($"a '.' with no digit after it at offset {at - 1}");
            }
        }

        if (at < text.Length && (text[at] == 'e' || text[at] == 'E'))
        {
            at++;
            if (at < text.Length && (text[at] == '+' || text[at] == '-'))
            {
                at++;
            }

            var exponent = 0;
            while (at < text.Length && text[at] >= '0' && text[at] <= '9')
            {
                at++;
                exponent++;
            }

            if (exponent == 0)
            {
                throw new FormatException($"an exponent with no digit at offset {at - 1}");
            }
        }

        var literal = text.Substring(start, at - start);
        if (!double.TryParse(
                literal,
                NumberStyles.Float,
                CultureInfo.InvariantCulture,
                out var value))
        {
            throw new FormatException($"'{literal}' is not a number this runtime can parse");
        }

        return new JsonValue { Kind = JsonKind.Number, Number = value };
    }

    private static void Expect(string text, ref int at, string word)
    {
        if (at + word.Length > text.Length
            || string.CompareOrdinal(text, at, word, 0, word.Length) != 0)
        {
            throw new FormatException($"expected '{word}' at offset {at}: '{Near(text, at)}'");
        }

        at += word.Length;
    }

    private static void SkipWhitespace(string text, ref int at)
    {
        while (at < text.Length
               && (text[at] == ' ' || text[at] == '\t' || text[at] == '\n' || text[at] == '\r'))
        {
            at++;
        }
    }

    private static char At(string text, int at)
    {
        if (at >= text.Length)
        {
            throw new FormatException("the document ends where more was expected");
        }

        return text[at];
    }

    private static string Near(string text, int at)
    {
        var start = Math.Max(0, at);
        var length = Math.Min(24, text.Length - start);
        return length <= 0 ? "<end of document>" : text.Substring(start, length);
    }
}
