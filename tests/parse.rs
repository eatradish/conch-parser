extern crate shell_lang;

use std::rc::Rc;

use shell_lang::ast::*;
use shell_lang::ast::builder::*;
use shell_lang::ast::Command::*;
use shell_lang::ast::ComplexWord::*;
use shell_lang::ast::CompoundCommandKind::*;
use shell_lang::ast::PipeableCommand::*;
use shell_lang::ast::SimpleWord::*;
use shell_lang::parse::*;
use shell_lang::parse::ParseError::*;
use shell_lang::token::Token;

mod parse_support;
use parse_support::*;

#[test]
fn test_linebreak_valid_with_comments_and_whitespace() {
    let mut p = make_parser("\n\t\t\t\n # comment1\n#comment2\n   \n");
    assert_eq!(p.linebreak(), vec!(
            Newline(None),
            Newline(None),
            Newline(Some(String::from("# comment1"))),
            Newline(Some(String::from("#comment2"))),
            Newline(None)
        )
    );
}

#[test]
fn test_linebreak_valid_empty() {
    let mut p = make_parser("");
    assert_eq!(p.linebreak(), vec!());
}

#[test]
fn test_linebreak_valid_nonnewline() {
    let mut p = make_parser("hello world");
    assert_eq!(p.linebreak(), vec!());
}

#[test]
fn test_linebreak_valid_eof_instead_of_newline() {
    let mut p = make_parser("#comment");
    assert_eq!(p.linebreak(), vec!(Newline(Some(String::from("#comment")))));
}

#[test]
fn test_linebreak_single_quote_insiginificant() {
    let mut p = make_parser("#unclosed quote ' comment");
    assert_eq!(p.linebreak(), vec!(Newline(Some(String::from("#unclosed quote ' comment")))));
}

#[test]
fn test_linebreak_double_quote_insiginificant() {
    let mut p = make_parser("#unclosed quote \" comment");
    assert_eq!(p.linebreak(), vec!(Newline(Some(String::from("#unclosed quote \" comment")))));
}

#[test]
fn test_linebreak_escaping_newline_insignificant() {
    let mut p = make_parser("#comment escapes newline\\\n");
    assert_eq!(p.linebreak(), vec!(Newline(Some(String::from("#comment escapes newline\\")))));
}

#[test]
fn test_skip_whitespace_preserve_newline() {
    let mut p = make_parser("    \t\t \t \t\n   ");
    p.skip_whitespace();
    assert_eq!(p.linebreak().len(), 1);
}

#[test]
fn test_skip_whitespace_preserve_comments() {
    let mut p = make_parser("    \t\t \t \t#comment\n   ");
    p.skip_whitespace();
    assert_eq!(p.linebreak().pop().unwrap(), Newline(Some(String::from("#comment"))));
}

#[test]
fn test_skip_whitespace_comments_capture_all_up_to_newline() {
    let mut p = make_parser("#comment&&||;;()<<-\n");
    assert_eq!(p.linebreak().pop().unwrap(), Newline(Some(String::from("#comment&&||;;()<<-"))));
}

#[test]
fn test_skip_whitespace_comments_may_end_with_eof() {
    let mut p = make_parser("#comment");
    assert_eq!(p.linebreak().pop().unwrap(), Newline(Some(String::from("#comment"))));
}

#[test]
fn test_skip_whitespace_skip_escapes_dont_affect_newlines() {
    let mut p = make_parser("  \t \\\n  \\\n#comment\n");
    p.skip_whitespace();
    assert_eq!(p.linebreak().pop().unwrap(), Newline(Some(String::from("#comment"))));
}

#[test]
fn test_skip_whitespace_skips_escaped_newlines() {
    let mut p = make_parser("\\\n\\\n   \\\n");
    p.skip_whitespace();
    assert_eq!(p.linebreak(), vec!());
}

#[test]
fn test_and_or_correct_associativity() {
    let mut p = make_parser("foo || bar && baz");
    let correct = CommandList {
        first: ListableCommand::Single(Simple(cmd_simple("foo"))),
        rest: vec!(
            AndOr::Or(ListableCommand::Single(Simple(cmd_simple("bar")))),
            AndOr::And(ListableCommand::Single(Simple(cmd_simple("baz")))),
        ),
    };
    assert_eq!(correct, p.and_or_list().unwrap());
}

#[test]
fn test_and_or_valid_with_newlines_after_operator() {
    let mut p = make_parser("foo ||\n\n\n\nbar && baz");
    let correct = CommandList {
        first: ListableCommand::Single(Simple(cmd_simple("foo"))),
        rest: vec!(
            AndOr::Or(ListableCommand::Single(Simple(cmd_simple("bar")))),
            AndOr::And(ListableCommand::Single(Simple(cmd_simple("baz")))),
        ),
    };
    assert_eq!(correct, p.and_or_list().unwrap());
}

#[test]
fn test_and_or_invalid_with_newlines_before_operator() {
    let mut p = make_parser("foo || bar\n\n&& baz");
    p.and_or_list().unwrap(); // Successful parse Or(foo, bar)
    // Fail to parse "&& baz" which is an error
    assert_eq!(Err(Unexpected(Token::AndIf, src(12, 3, 1))), p.complete_command());
}

#[test]
fn test_pipeline_valid_bang() {
    let mut p = make_parser("! foo | bar | baz");
    let correct = CommandList {
        first: ListableCommand::Pipe(true, vec!(
            Simple(cmd_simple("foo")),
            Simple(cmd_simple("bar")),
            Simple(cmd_simple("baz")),
        )),
        rest: vec!(),
    };
    assert_eq!(correct, p.and_or_list().unwrap());
}

#[test]
fn test_pipeline_valid_bangs_in_and_or() {
    let mut p = make_parser("! foo | bar || ! baz && ! foobar");
    let correct = CommandList {
        first: ListableCommand::Pipe(true, vec!(
            Simple(cmd_simple("foo")),
            Simple(cmd_simple("bar"))
        )),
        rest: vec!(
            AndOr::Or(ListableCommand::Pipe(true, vec!(
                Simple(cmd_simple("baz")),
            ))),
            AndOr::And(ListableCommand::Pipe(true, vec!(
                Simple(cmd_simple("foobar")),
            ))),
        ),
    };
    assert_eq!(correct, p.and_or_list().unwrap());
}

#[test]
fn test_pipeline_no_bang_single_cmd_optimize_wrapper_out() {
    let mut p = make_parser("foo");
    let parse = p.pipeline().unwrap();
    if let ListableCommand::Pipe(..) = parse {
        panic!("Parser::pipeline should not create a wrapper if no ! present and only a single command");
    }
}

#[test]
fn test_pipeline_invalid_multiple_bangs_in_same_pipeline() {
    let mut p = make_parser("! foo | bar | ! baz");
    assert_eq!(Err(Unexpected(Token::Bang, src(14, 1, 15))), p.pipeline());
}

#[test]
fn test_comment_cannot_start_mid_whitespace_delimited_word() {
    let mut p = make_parser("hello#world");
    let w = p.word().unwrap().expect("no valid word was discovered");
    assert_eq!(w, word("hello#world"));
}

#[test]
fn test_comment_can_start_if_whitespace_before_pound() {
    let mut p = make_parser("hello #world");
    p.word().unwrap().expect("no valid word was discovered");
    let comment = p.linebreak();
    assert_eq!(comment, vec!(Newline(Some(String::from("#world")))));
}

#[test]
fn test_complete_command_job() {
    let mut p = make_parser("foo && bar & baz");
    let cmd1 = p.complete_command().unwrap().expect("failed to parse first command");
    let cmd2 = p.complete_command().unwrap().expect("failed to parse second command");

    let correct1 = TopLevelCommand(Job(CommandList {
        first: ListableCommand::Single(Simple(cmd_simple("foo"))),
        rest: vec!(
            AndOr::And(ListableCommand::Single(Simple(cmd_simple("bar"))))
        ),
    }));
    let correct2 = cmd("baz");

    assert_eq!(correct1, cmd1);
    assert_eq!(correct2, cmd2);
}

#[test]
fn test_complete_command_non_eager_parse() {
    let mut p = make_parser("foo && bar; baz\n\nqux");
    let cmd1 = p.complete_command().unwrap().expect("failed to parse first command");
    let cmd2 = p.complete_command().unwrap().expect("failed to parse second command");
    let cmd3 = p.complete_command().unwrap().expect("failed to parse third command");

    let correct1 = TopLevelCommand(List(CommandList {
        first: ListableCommand::Single(Simple(cmd_simple("foo"))),
        rest: vec!(
            AndOr::And(ListableCommand::Single(Simple(cmd_simple("bar"))))
        ),
    }));
    let correct2 = cmd("baz");
    let correct3 = cmd("qux");

    assert_eq!(correct1, cmd1);
    assert_eq!(correct2, cmd2);
    assert_eq!(correct3, cmd3);
}

#[test]
fn test_complete_command_valid_no_input() {
    let mut p = make_parser("");
    p.complete_command().expect("no input caused an error");
}

#[test]
fn test_simple_command_valid_assignments_at_start_of_command() {
    let mut p = make_parser("var=val ENV=true BLANK= foo bar baz");
    let SimpleCommandFragments { cmd, vars, ..} = sample_simple_command();
    let correct = Simple(Box::new(SimpleCommand { cmd: cmd, vars: vars, io: vec!() }));
    assert_eq!(correct, p.simple_command().unwrap());
}

#[test]
fn test_simple_command_assignments_after_start_of_command_should_be_args() {
    let mut p = make_parser("var=val ENV=true BLANK= foo var2=val2 bar baz var3=val3");
    let SimpleCommandFragments { cmd, vars, ..} = sample_simple_command();
    let (cmd, mut args) = cmd.unwrap();
    args.insert(0, word("var2=val2"));
    args.push(word("var3=val3"));
    let correct = Simple(Box::new(SimpleCommand { cmd: Some((cmd, args)), vars: vars, io: vec!() }));
    assert_eq!(correct, p.simple_command().unwrap());
}

#[test]
fn test_simple_command_redirections_at_start_of_command() {
    let mut p = make_parser("2>|clob 3<>rw <in var=val ENV=true BLANK= foo bar baz");
    let SimpleCommandFragments { cmd, vars, io } = sample_simple_command();
    let correct = Simple(Box::new(SimpleCommand { cmd: cmd, vars: vars, io: io }));
    assert_eq!(correct, p.simple_command().unwrap());
}

#[test]
fn test_simple_command_redirections_at_end_of_command() {
    let mut p = make_parser("var=val ENV=true BLANK= foo bar baz 2>|clob 3<>rw <in");
    let SimpleCommandFragments { cmd, vars, io } = sample_simple_command();
    let correct = Simple(Box::new(SimpleCommand { cmd: cmd, vars: vars, io: io }));
    assert_eq!(correct, p.simple_command().unwrap());
}

#[test]
fn test_simple_command_redirections_throughout_the_command() {
    let mut p = make_parser("2>|clob var=val 3<>rw ENV=true BLANK= foo bar <in baz 4>&-");
    let SimpleCommandFragments { cmd, vars, mut io } = sample_simple_command();
    io.push(Redirect::DupWrite(Some(4), word("-")));
    let correct = Simple(Box::new(SimpleCommand { cmd: cmd, vars: vars, io: io }));
    assert_eq!(correct, p.simple_command().unwrap());
}

#[test]
fn test_do_group_valid() {
    let mut p = make_parser("do foo\nbar; baz\n#comment\n done");
    let correct = CommandGroup {
        commands: vec!(cmd("foo"), cmd("bar"), cmd("baz")),
        trailing_comments: vec!(Newline(Some("#comment".into()))),
    };
    assert_eq!(correct, p.do_group().unwrap());
}

#[test]
fn test_do_group_invalid_missing_separator() {
    let mut p = make_parser("do foo\nbar; baz done");
    assert_eq!(Err(IncompleteCmd("do", src(0,1,1), "done", src(20,2,14))), p.do_group());
}

#[test]
fn test_do_group_valid_keyword_delimited_by_separator() {
    let mut p = make_parser("do foo done; done");
    let correct = CommandGroup {
        commands: vec!(cmd_args("foo", &["done"])),
        trailing_comments: vec!(),
    };
    assert_eq!(correct, p.do_group().unwrap());
}

#[test]
fn test_do_group_invalid_missing_keyword() {
    let mut p = make_parser("foo\nbar; baz; done");
    assert_eq!(Err(Unexpected(Token::Name(String::from("foo")), src(0,1,1))), p.do_group());
    let mut p = make_parser("do foo\nbar; baz");
    assert_eq!(Err(IncompleteCmd("do", src(0,1,1), "done", src(15,2,9))), p.do_group());
}

#[test]
fn test_do_group_invalid_quoted() {
    let cmds = [
        ("'do' foo\nbar; baz; done",   Unexpected(Token::SingleQuote, src(0, 1, 1))),
        ("do foo\nbar; baz; 'done'",   IncompleteCmd("do", src(0,1,1), "done", src(23,2,17))),
        ("\"do\" foo\nbar; baz; done", Unexpected(Token::DoubleQuote, src(0, 1, 1))),
        ("do foo\nbar; baz; \"done\"", IncompleteCmd("do", src(0,1,1), "done", src(23,2,17))),
    ];

    for &(c, ref e) in cmds.into_iter() {
        match make_parser(c).do_group() {
            Ok(result) => panic!("Unexpectedly parsed \"{}\" as\n{:#?}", c, result),
            Err(ref err) => if err != e {
                panic!("Expected the source \"{}\" to return the error `{:?}`, but got `{:?}`",
                       c, e, err);
            },
        }
    }
}

#[test]
fn test_do_group_invalid_concat() {
    let mut p = make_parser_from_tokens(vec!(
        Token::Literal(String::from("d")),
        Token::Literal(String::from("o")),
        Token::Newline,
        Token::Literal(String::from("foo")),
        Token::Newline,
        Token::Literal(String::from("done")),
    ));
    assert_eq!(Err(Unexpected(Token::Literal(String::from("d")), src(0,1,1))), p.do_group());
    let mut p = make_parser_from_tokens(vec!(
        Token::Literal(String::from("do")),
        Token::Newline,
        Token::Literal(String::from("foo")),
        Token::Newline,
        Token::Literal(String::from("do")),
        Token::Literal(String::from("ne")),
    ));
    assert_eq!(Err(IncompleteCmd("do", src(0,1,1), "done", src(11,3,5))), p.do_group());
}

#[test]
fn test_do_group_should_recognize_literals_and_names() {
    for do_tok in vec!(Token::Literal(String::from("do")), Token::Name(String::from("do"))) {
        for done_tok in vec!(Token::Literal(String::from("done")), Token::Name(String::from("done"))) {
            let mut p = make_parser_from_tokens(vec!(
                do_tok.clone(),
                Token::Newline,
                Token::Literal(String::from("foo")),
                Token::Newline,
                done_tok
            ));
            p.do_group().unwrap();
        }
    }
}

#[test]
fn test_do_group_invalid_missing_body() {
    let mut p = make_parser("do\ndone");
    assert_eq!(Err(Unexpected(Token::Name("done".into()), src(3,2,1))), p.do_group());
}

#[test]
fn test_brace_group_valid() {
    let mut p = make_parser("{ foo\nbar; baz\n#comment1\n#comment2\n }");
    let correct = CommandGroup {
        commands: vec!(cmd("foo"), cmd("bar"), cmd("baz")),
        trailing_comments: vec!(
            Newline(Some("#comment1".into())),
            Newline(Some("#comment2".into())),
        ),
    };
    assert_eq!(correct, p.brace_group().unwrap());
}

#[test]
fn test_brace_group_invalid_missing_separator() {
    assert_eq!(Err(Unmatched(Token::CurlyOpen, src(0,1,1))), make_parser("{ foo\nbar; baz }").brace_group());
}

#[test]
fn test_brace_group_invalid_start_must_be_whitespace_delimited() {
    let mut p = make_parser("{foo\nbar; baz; }");
    assert_eq!(Err(Unexpected(Token::Name(String::from("foo")), src(1,1,2))), p.brace_group());
}

#[test]
fn test_brace_group_valid_end_must_be_whitespace_and_separator_delimited() {
    let mut p = make_parser("{ foo\nbar}; baz; }");
    p.brace_group().unwrap();
    assert_eq!(p.complete_command().unwrap(), None); // Ensure stream is empty
    let mut p = make_parser("{ foo\nbar; }baz; }");
    p.brace_group().unwrap();
    assert_eq!(p.complete_command().unwrap(), None); // Ensure stream is empty
}

#[test]
fn test_brace_group_valid_keyword_delimited_by_separator() {
    let mut p = make_parser("{ foo }; }");
    let correct = CommandGroup {
        commands: vec!(cmd_args("foo", &["}"])),
        trailing_comments: vec!(),
    };
    assert_eq!(correct, p.brace_group().unwrap());
}

#[test]
fn test_brace_group_invalid_missing_keyword() {
    let mut p = make_parser("{ foo\nbar; baz");
    assert_eq!(Err(Unmatched(Token::CurlyOpen, src(0,1,1))), p.brace_group());
    let mut p = make_parser("foo\nbar; baz; }");
    assert_eq!(Err(Unexpected(Token::Name(String::from("foo")), src(0,1,1))), p.brace_group());
}

#[test]
fn test_brace_group_invalid_quoted() {
    let cmds = [
        ("'{' foo\nbar; baz; }",   Unexpected(Token::SingleQuote, src(0,1,1))),
        ("{ foo\nbar; baz; '}'",   Unmatched(Token::CurlyOpen, src(0,1,1))),
        ("\"{\" foo\nbar; baz; }", Unexpected(Token::DoubleQuote, src(0,1,1))),
        ("{ foo\nbar; baz; \"}\"", Unmatched(Token::CurlyOpen, src(0,1,1))),
    ];

    for &(c, ref e) in cmds.into_iter() {
        match make_parser(c).brace_group() {
            Ok(result) => panic!("Unexpectedly parsed \"{}\" as\n{:#?}", c, result),
            Err(ref err) => if err != e {
                panic!("Expected the source \"{}\" to return the error `{:?}`, but got `{:?}`",
                       c, e, err);
            }
        }
    }
}

#[test]
fn test_brace_group_invalid_missing_body() {
    assert_eq!(Err(Unexpected(Token::CurlyClose, src(2, 2, 1))), make_parser("{\n}").brace_group());
}

#[test]
fn test_subshell_valid() {
    let mut p = make_parser("( foo\nbar; baz\n#comment\n )");
    let correct = CommandGroup {
        commands: vec!(cmd("foo"), cmd("bar"), cmd("baz")),
        trailing_comments: vec!(Newline(Some("#comment".into()))),
    };
    assert_eq!(correct, p.subshell().unwrap());
}

#[test]
fn test_subshell_valid_separator_not_needed() {
    let correct = CommandGroup {
        commands: vec!(cmd("foo")),
        trailing_comments: vec!(),
    };
    assert_eq!(correct, make_parser("( foo )").subshell().unwrap());

    let correct_with_comment = CommandGroup {
        commands: vec!(cmd("foo")),
        trailing_comments: vec!(Newline(Some("#comment".into()))),
    };
    assert_eq!(correct_with_comment, make_parser("( foo\n#comment\n )").subshell().unwrap());
}

#[test]
fn test_subshell_space_between_parens_not_needed() {
    let mut p = make_parser("(foo )");
    p.subshell().unwrap();
    let mut p = make_parser("( foo)");
    p.subshell().unwrap();
    let mut p = make_parser("(foo)");
    p.subshell().unwrap();
}

#[test]
fn test_subshell_invalid_missing_keyword() {
    assert_eq!(Err(Unmatched(Token::ParenOpen, src(0,1,1))), make_parser("( foo\nbar; baz").subshell());
    assert_eq!(Err(Unexpected(Token::Name(String::from("foo")), src(0,1,1))),
        make_parser("foo\nbar; baz; )").subshell());
}

#[test]
fn test_subshell_invalid_quoted() {
    let cmds = [
        ("'(' foo\nbar; baz; )",   Unexpected(Token::SingleQuote, src(0,1,1))),
        ("( foo\nbar; baz; ')'",   Unmatched(Token::ParenOpen, src(0,1,1))),
        ("\"(\" foo\nbar; baz; )", Unexpected(Token::DoubleQuote, src(0,1,1))),
        ("( foo\nbar; baz; \")\"", Unmatched(Token::ParenOpen, src(0,1,1))),
    ];

    for &(c, ref e) in cmds.into_iter() {
        match make_parser(c).subshell() {
            Ok(result) => panic!("Unexpectedly parsed \"{}\" as\n{:#?}", c, result),
            Err(ref err) => if err != e {
                panic!("Expected the source \"{}\" to return the error `{:?}`, but got `{:?}`",
                       c, e, err);
            },
        }
    }
}

#[test]
fn test_subshell_invalid_missing_body() {
    assert_eq!(Err(Unexpected(Token::ParenClose, src(2,2,1))), make_parser("(\n)").subshell());
    assert_eq!(Err(Unexpected(Token::ParenClose, src(1,1,2))), make_parser("()").subshell());
}

#[test]
fn test_loop_command_while_valid() {
    let mut p = make_parser("while guard1; guard2;\n#guard_comment\n do foo\nbar; baz\n#body_comment\n done");
    let (until, GuardBodyPairGroup { guard, body }) = p.loop_command().unwrap();

    let correct_guard = CommandGroup {
        commands: vec!(cmd("guard1"), cmd("guard2")),
        trailing_comments: vec!(Newline(Some("#guard_comment".into()))),
    };
    let correct_body = CommandGroup {
        commands: vec!(cmd("foo"), cmd("bar"), cmd("baz")),
        trailing_comments: vec!(Newline(Some("#body_comment".into()))),
    };

    assert_eq!(until, LoopKind::While);
    assert_eq!(correct_guard, guard);
    assert_eq!(correct_body, body);
}

#[test]
fn test_loop_command_until_valid() {
    let mut p = make_parser("until guard1; guard2;\n#guard_comment\n do foo\nbar; baz\n#body_comment\n done");
    let (until, GuardBodyPairGroup { guard, body }) = p.loop_command().unwrap();

    let correct_guard = CommandGroup {
        commands: vec!(cmd("guard1"), cmd("guard2")),
        trailing_comments: vec!(Newline(Some("#guard_comment".into()))),
    };
    let correct_body = CommandGroup {
        commands: vec!(cmd("foo"), cmd("bar"), cmd("baz")),
        trailing_comments: vec!(Newline(Some("#body_comment".into()))),
    };

    assert_eq!(until, LoopKind::Until);
    assert_eq!(correct_guard, guard);
    assert_eq!(correct_body, body);
}

#[test]
fn test_loop_command_invalid_missing_separator() {
    let mut p = make_parser("while guard do foo\nbar; baz; done");
    assert_eq!(Err(IncompleteCmd("while", src(0,1,1), "do", src(33,2,15))), p.loop_command());
    let mut p = make_parser("while guard; do foo\nbar; baz done");
    assert_eq!(Err(IncompleteCmd("do", src(13,1,14), "done", src(33,2,14))), p.loop_command());
}

#[test]
fn test_loop_command_invalid_missing_keyword() {
    let mut p = make_parser("guard; do foo\nbar; baz; done");
    assert_eq!(Err(Unexpected(Token::Name(String::from("guard")), src(0,1,1))), p.loop_command());
}

#[test]
fn test_loop_command_invalid_missing_guard() {
    // With command separator between loop and do keywords
    let mut p = make_parser("while; do foo\nbar; baz; done");
    assert_eq!(Err(Unexpected(Token::Semi, src(5, 1, 6))), p.loop_command());
    let mut p = make_parser("until; do foo\nbar; baz; done");
    assert_eq!(Err(Unexpected(Token::Semi, src(5, 1, 6))), p.loop_command());

    // Without command separator between loop and do keywords
    let mut p = make_parser("while do foo\nbar; baz; done");
    assert_eq!(Err(Unexpected(Token::Name(String::from("do")), src(6, 1, 7))), p.loop_command());
    let mut p = make_parser("until do foo\nbar; baz; done");
    assert_eq!(Err(Unexpected(Token::Name(String::from("do")), src(6, 1, 7))), p.loop_command());
}

#[test]
fn test_loop_command_invalid_quoted() {
    let cmds = [
        ("'while' guard do foo\nbar; baz; done",   Unexpected(Token::SingleQuote, src(0,1,1))),
        ("'until' guard do foo\nbar; baz; done",   Unexpected(Token::SingleQuote, src(0,1,1))),
        ("\"while\" guard do foo\nbar; baz; done", Unexpected(Token::DoubleQuote, src(0,1,1))),
        ("\"until\" guard do foo\nbar; baz; done", Unexpected(Token::DoubleQuote, src(0,1,1))),
    ];

    for &(c, ref e) in cmds.into_iter() {
        match make_parser(c).loop_command() {
            Ok(result) => panic!("Unexpectedly parsed \"{}\" as\n{:#?}", c, result),
            Err(ref err) => if err != e {
                panic!("Expected the source \"{}\" to return the error `{:?}`, but got `{:?}`",
                       c, e, err);
            },
        }
    }
}

#[test]
fn test_loop_command_invalid_concat() {
    let mut p = make_parser_from_tokens(vec!(
        Token::Literal(String::from("whi")),
        Token::Literal(String::from("le")),
        Token::Newline,
        Token::Literal(String::from("guard")),
        Token::Newline,
        Token::Literal(String::from("do")),
        Token::Literal(String::from("foo")),
        Token::Newline,
        Token::Literal(String::from("done")),
    ));
    assert_eq!(Err(Unexpected(Token::Literal(String::from("whi")), src(0,1,1))), p.loop_command());
    let mut p = make_parser_from_tokens(vec!(
        Token::Literal(String::from("un")),
        Token::Literal(String::from("til")),
        Token::Newline,
        Token::Literal(String::from("guard")),
        Token::Newline,
        Token::Literal(String::from("do")),
        Token::Literal(String::from("foo")),
        Token::Newline,
        Token::Literal(String::from("done")),
    ));
    assert_eq!(Err(Unexpected(Token::Literal(String::from("un")), src(0,1,1))), p.loop_command());
}

#[test]
fn test_loop_command_should_recognize_literals_and_names() {
    for kw in vec!(
        Token::Literal(String::from("while")),
        Token::Name(String::from("while")),
        Token::Literal(String::from("until")),
        Token::Name(String::from("until")))
    {
        let mut p = make_parser_from_tokens(vec!(
            kw,
            Token::Newline,
            Token::Literal(String::from("guard")),
            Token::Newline,
            Token::Literal(String::from("do")),
            Token::Newline,
            Token::Literal(String::from("foo")),
            Token::Newline,
            Token::Literal(String::from("done")),
        ));
        p.loop_command().unwrap();
    }
}

#[test]
fn test_if_command_valid_with_else() {
    let guard1 = cmd("guard1");
    let guard2 = cmd("guard2");
    let guard3 = cmd("guard3");

    let body1 = cmd("body1");
    let body2 = cmd("body2");

    let els = cmd("else");

    let correct = IfFragments {
        conditionals: vec!(
            GuardBodyPairGroup {
                guard: CommandGroup {
                    commands: vec!(guard1, guard2),
                    trailing_comments: vec!(Newline(Some("#guard_comment_a".into()))),
                },
                body: CommandGroup {
                    commands: vec!(body1),
                    trailing_comments: vec!(Newline(Some("#body_comment_a".into()))),
                },
            },
            GuardBodyPairGroup {
                guard: CommandGroup {
                    commands: vec!(guard3),
                    trailing_comments: vec!(Newline(Some("#guard_comment_b".into()))),
                },
                body: CommandGroup {
                    commands: vec!(body2),
                    trailing_comments: vec!(Newline(Some("#body_comment_b".into()))),
                },
            },
        ),
        else_branch: Some(CommandGroup {
            commands: vec!(els),
            trailing_comments: vec!(Newline(Some("#else_comment".into()))),
        }),
    };
    let mut p = make_parser("\
        if guard1; guard2;
        #guard_comment_a
        then body1
        #body_comment_a
        elif guard3;
        #guard_comment_b
        then body2;
        #body_comment_b
        else else;
        #else_comment
        fi
    ");
    assert_eq!(correct, p.if_command().unwrap());
}

#[test]
fn test_if_command_valid_without_else() {
    let guard1 = cmd("guard1");
    let guard2 = cmd("guard2");
    let guard3 = cmd("guard3");

    let body1 = cmd("body1");
    let body2 = cmd("body2");

    let correct = IfFragments {
        conditionals: vec!(
            GuardBodyPairGroup {
                guard: CommandGroup {
                    commands: vec!(guard1, guard2),
                    trailing_comments: vec!(Newline(Some("#guard_comment_a".into()))),
                },
                body: CommandGroup {
                    commands: vec!(body1),
                    trailing_comments: vec!(Newline(Some("#body_comment_a".into()))),
                },
            },
            GuardBodyPairGroup {
                guard: CommandGroup {
                    commands: vec!(guard3),
                    trailing_comments: vec!(Newline(Some("#guard_comment_b".into()))),
                },
                body: CommandGroup {
                    commands: vec!(body2),
                    trailing_comments: vec!(Newline(Some("#body_comment_b".into()))),
                },
            },
        ),
        else_branch: None,
    };
    let mut p = make_parser("\
        if guard1; guard2;
        #guard_comment_a
        then body1
        #body_comment_a
        elif guard3;
        #guard_comment_b
        then body2;
        #body_comment_b
        fi
    ");
    assert_eq!(correct, p.if_command().unwrap());
}

#[test]
fn test_if_command_invalid_missing_separator() {
    let mut p = make_parser("if guard; then body1; elif guard2; then body2; else else fi");
    assert_eq!(Err(IncompleteCmd("if", src(0,1,1), "fi", src(59,1,60))), p.if_command());
}

#[test]
fn test_if_command_invalid_missing_keyword() {
    let mut p = make_parser("guard1; then body1; elif guard2; then body2; else else; fi");
    assert_eq!(Err(Unexpected(Token::Name(String::from("guard1")), src(0,1,1))), p.if_command());
    let mut p = make_parser("if guard1; then body1; elif guard2; then body2; else else;");
    assert_eq!(Err(IncompleteCmd("if", src(0,1,1), "fi", src(58,1,59))), p.if_command());
}

#[test]
fn test_if_command_invalid_missing_guard() {
    let mut p = make_parser("if; then body1; elif guard2; then body2; else else; fi");
    assert_eq!(Err(Unexpected(Token::Semi, src(2,1,3))), p.if_command());
}

#[test]
fn test_if_command_invalid_missing_body() {
    let mut p = make_parser("if guard; then; elif guard2; then body2; else else; fi");
    assert_eq!(Err(Unexpected(Token::Semi, src(14, 1, 15))), p.if_command());
    let mut p = make_parser("if guard1; then body1; elif; then body2; else else; fi");
    assert_eq!(Err(Unexpected(Token::Semi, src(27, 1, 28))), p.if_command());
    let mut p = make_parser("if guard1; then body1; elif guard2; then body2; else; fi");
    assert_eq!(Err(Unexpected(Token::Semi, src(52, 1, 53))), p.if_command());
}

#[test]
fn test_if_command_invalid_quoted() {
    let cmds = [
        ("'if' guard1; then body1; elif guard2; then body2; else else; fi", Unexpected(Token::SingleQuote, src(0,1,1))),
        ("if guard1; then body1; elif guard2; then body2; else else; 'fi'", IncompleteCmd("if", src(0,1,1), "fi", src(63,1,64))),
        ("\"if\" guard1; then body1; elif guard2; then body2; else else; fi", Unexpected(Token::DoubleQuote, src(0,1,1))),
        ("if guard1; then body1; elif guard2; then body2; else else; \"fi\"", IncompleteCmd("if", src(0,1,1), "fi", src(63,1,64))),
    ];

    for &(s, ref e) in cmds.into_iter() {
        match make_parser(s).if_command() {
            Ok(result) => panic!("Unexpectedly parsed \"{}\" as\n{:#?}", s, result),
            Err(ref err) => if err != e {
                panic!("Expected the source \"{}\" to return the error `{:?}`, but got `{:?}`",
                       s, e, err);
            },
        }
    }
}

#[test]
fn test_if_command_invalid_concat() {
    let mut p = make_parser_from_tokens(vec!(
        Token::Literal(String::from("i")), Token::Literal(String::from("f")),
        Token::Newline, Token::Literal(String::from("guard1")), Token::Newline,
        Token::Literal(String::from("then")),
        Token::Newline, Token::Literal(String::from("body1")), Token::Newline,
        Token::Literal(String::from("elif")),
        Token::Newline, Token::Literal(String::from("guard2")), Token::Newline,
        Token::Literal(String::from("then")),
        Token::Newline, Token::Literal(String::from("body2")), Token::Newline,
        Token::Literal(String::from("else")),
        Token::Newline, Token::Literal(String::from("else part")), Token::Newline,
        Token::Literal(String::from("fi")),
    ));
    assert_eq!(Err(Unexpected(Token::Literal(String::from("i")), src(0,1,1))), p.if_command());

    // Splitting up `then`, `elif`, and `else` tokens makes them
    // get interpreted as arguments, so an explicit error may not be raised
    // although the command will be malformed

    let mut p = make_parser_from_tokens(vec!(
        Token::Literal(String::from("if")),
        Token::Newline, Token::Literal(String::from("guard1")), Token::Newline,
        Token::Literal(String::from("then")),
        Token::Newline, Token::Literal(String::from("body1")), Token::Newline,
        Token::Literal(String::from("elif")),
        Token::Newline, Token::Literal(String::from("guard2")), Token::Newline,
        Token::Literal(String::from("then")),
        Token::Newline, Token::Literal(String::from("body2")), Token::Newline,
        Token::Literal(String::from("else")),
        Token::Newline, Token::Literal(String::from("else part")), Token::Newline,
        Token::Literal(String::from("f")), Token::Literal(String::from("i")),
    ));
    assert_eq!(Err(IncompleteCmd("if", src(0,1,1), "fi", src(61,11,3))), p.if_command());
}

#[test]
fn test_if_command_should_recognize_literals_and_names() {
    for if_tok in vec!(Token::Literal(String::from("if")), Token::Name(String::from("if"))) {
        for then_tok in vec!(Token::Literal(String::from("then")), Token::Name(String::from("then"))) {
            for elif_tok in vec!(Token::Literal(String::from("elif")), Token::Name(String::from("elif"))) {
                for else_tok in vec!(Token::Literal(String::from("else")), Token::Name(String::from("else"))) {
                    for fi_tok in vec!(Token::Literal(String::from("fi")), Token::Name(String::from("fi"))) {
                        let mut p = make_parser_from_tokens(vec!(
                                if_tok.clone(),
                                Token::Whitespace(String::from(" ")),

                                Token::Literal(String::from("guard1")),
                                Token::Newline,

                                then_tok.clone(),
                                Token::Newline,
                                Token::Literal(String::from("body1")),

                                elif_tok.clone(),
                                Token::Whitespace(String::from(" ")),

                                Token::Literal(String::from("guard2")),
                                Token::Newline,
                                then_tok.clone(),
                                Token::Whitespace(String::from(" ")),
                                Token::Literal(String::from("body2")),

                                else_tok.clone(),
                                Token::Whitespace(String::from(" ")),

                                Token::Whitespace(String::from(" ")),
                                Token::Literal(String::from("else part")),
                                Token::Newline,

                                fi_tok,
                        ));
                        p.if_command().unwrap();
                    }
                }
            }
        }
    }
}

#[test]
fn test_braces_literal_unless_brace_group_expected() {
    let source = "echo {} } {";
    let mut p = make_parser(source);
    assert_eq!(p.word().unwrap().unwrap(), word("echo"));
    assert_eq!(p.word().unwrap().unwrap(), word("{}"));
    assert_eq!(p.word().unwrap().unwrap(), word("}"));
    assert_eq!(p.word().unwrap().unwrap(), word("{"));

    let correct = Some(cmd_args("echo", &["{}", "}", "{"]));
    assert_eq!(correct, make_parser(source).complete_command().unwrap());
}

#[test]
fn test_for_command_valid_with_words() {
    let mut p = make_parser("\
    for var #var comment
    #prew1
    #prew2
    in one two three #word comment
    #precmd1
    #precmd2
    do echo;
    #body_comment
    done
    ");
    assert_eq!(p.for_command(), Ok(ForFragments {
        var: "var".into(),
        var_comment: Some(Newline(Some("#var comment".into()))),
        words: Some((
            vec!(
                Newline(Some("#prew1".into())),
                Newline(Some("#prew2".into())),
            ),
            vec!(
                word("one"),
                word("two"),
                word("three"),
            ),
            Some(Newline(Some("#word comment".into())))
        )),
        pre_body_comments: vec!(
            Newline(Some("#precmd1".into())),
            Newline(Some("#precmd2".into())),
        ),
        body: CommandGroup {
            commands: vec!(cmd("echo")),
            trailing_comments: vec!(Newline(Some("#body_comment".into()))),
        },
    }));
}

#[test]
fn test_for_command_valid_without_words() {
    let mut p = make_parser("\
    for var #var comment
    #w1
    #w2
    do echo;
    #body_comment
    done
    ");
    assert_eq!(p.for_command(), Ok(ForFragments {
        var: "var".into(),
        var_comment: Some(Newline(Some("#var comment".into()))),
        words: None,
        pre_body_comments: vec!(
            Newline(Some("#w1".into())),
            Newline(Some("#w2".into())),
        ),
        body: CommandGroup {
            commands: vec!(cmd("echo")),
            trailing_comments: vec!(Newline(Some("#body_comment".into()))),
        },
    }));
}

#[test]
fn test_for_command_valid_with_in_but_no_words_with_separator() {
    let mut p = make_parser("for var in\ndo echo $var; done");
    p.for_command().unwrap();
    let mut p = make_parser("for var in;do echo $var; done");
    p.for_command().unwrap();
}

#[test]
fn test_for_command_valid_with_separator() {
    let mut p = make_parser("for var in one two three\ndo echo $var; done");
    p.for_command().unwrap();
    let mut p = make_parser("for var in one two three;do echo $var; done");
    p.for_command().unwrap();
}

#[test]
fn test_for_command_invalid_with_in_no_words_no_with_separator() {
    let mut p = make_parser("for var in do echo $var; done");
    assert_eq!(Err(IncompleteCmd("for", src(0,1,1), "do", src(25,1,26))), p.for_command());
}

#[test]
fn test_for_command_invalid_missing_separator() {
    let mut p = make_parser("for var in one two three do echo $var; done");
    assert_eq!(Err(IncompleteCmd("for", src(0,1,1), "do", src(39,1,40))), p.for_command());
}

#[test]
fn test_for_command_invalid_amp_not_valid_separator() {
    let mut p = make_parser("for var in one two three& do echo $var; done");
    assert_eq!(Err(Unexpected(Token::Amp, src(24, 1, 25))), p.for_command());
}

#[test]
fn test_for_command_invalid_missing_keyword() {
    let mut p = make_parser("var in one two three\ndo echo $var; done");
    assert_eq!(Err(Unexpected(Token::Name(String::from("var")), src(0,1,1))), p.for_command());
}

#[test]
fn test_for_command_invalid_missing_var() {
    let mut p = make_parser("for in one two three\ndo echo $var; done");
    assert_eq!(Err(IncompleteCmd("for", src(0,1,1), "in", src(7,1,8))), p.for_command());
}

#[test]
fn test_for_command_invalid_missing_body() {
    let mut p = make_parser("for var in one two three\n");
    assert_eq!(Err(IncompleteCmd("for", src(0,1,1), "do", src(25,2,1))), p.for_command());
}

#[test]
fn test_for_command_invalid_quoted() {
    let cmds = [
        ("'for' var in one two three\ndo echo $var; done", Unexpected(Token::SingleQuote, src(0,1,1))),
        ("for var 'in' one two three\ndo echo $var; done", IncompleteCmd("for", src(0,1,1), "in", src(8,1,9))),
        ("\"for\" var in one two three\ndo echo $var; done", Unexpected(Token::DoubleQuote, src(0,1,1))),
        ("for var \"in\" one two three\ndo echo $var; done", IncompleteCmd("for", src(0,1,1), "in", src(8,1,9))),
    ];

    for &(c, ref e) in cmds.into_iter() {
        match make_parser(c).for_command() {
            Ok(result) => panic!("Unexpectedly parsed \"{}\" as\n{:#?}", c, result),
            Err(ref err) => if err != e {
                panic!("Expected the source \"{}\" to return the error `{:?}`, but got `{:?}`",
                       c, e, err);
            },
        }
    }
}

#[test]
fn test_for_command_invalid_var_must_be_name() {
    let mut p = make_parser("for 123var in one two three\ndo echo $var; done");
    assert_eq!(Err(BadIdent(String::from("123var"), src(4, 1, 5))), p.for_command());
    let mut p = make_parser("for 'var' in one two three\ndo echo $var; done");
    assert_eq!(Err(Unexpected(Token::SingleQuote, src(4, 1, 5))), p.for_command());
    let mut p = make_parser("for \"var\" in one two three\ndo echo $var; done");
    assert_eq!(Err(Unexpected(Token::DoubleQuote, src(4, 1, 5))), p.for_command());
    let mut p = make_parser("for var*% in one two three\ndo echo $var; done");
    assert_eq!(Err(Unexpected(Token::Star, src(7, 1, 8))), p.for_command());
}

#[test]
fn test_for_command_invalid_concat() {
    let mut p = make_parser_from_tokens(vec!(
        Token::Literal(String::from("fo")), Token::Literal(String::from("r")),
        Token::Whitespace(String::from(" ")), Token::Name(String::from("var")),
        Token::Whitespace(String::from(" ")),

        Token::Literal(String::from("in")),
        Token::Literal(String::from("one")), Token::Whitespace(String::from(" ")),
        Token::Literal(String::from("two")), Token::Whitespace(String::from(" ")),
        Token::Literal(String::from("three")), Token::Whitespace(String::from(" ")),
        Token::Newline,

        Token::Literal(String::from("do")), Token::Whitespace(String::from(" ")),
        Token::Literal(String::from("echo")), Token::Whitespace(String::from(" ")),
        Token::Dollar, Token::Literal(String::from("var")),
        Token::Newline,
        Token::Literal(String::from("done")),
    ));
    assert_eq!(Err(Unexpected(Token::Literal(String::from("fo")), src(0, 1, 1))), p.for_command());

    let mut p = make_parser_from_tokens(vec!(
        Token::Literal(String::from("for")),
        Token::Whitespace(String::from(" ")), Token::Name(String::from("var")),
        Token::Whitespace(String::from(" ")),

        Token::Literal(String::from("i")), Token::Literal(String::from("n")),
        Token::Literal(String::from("one")), Token::Whitespace(String::from(" ")),
        Token::Literal(String::from("two")), Token::Whitespace(String::from(" ")),
        Token::Literal(String::from("three")), Token::Whitespace(String::from(" ")),
        Token::Newline,

        Token::Literal(String::from("do")), Token::Whitespace(String::from(" ")),
        Token::Literal(String::from("echo")), Token::Whitespace(String::from(" ")),
        Token::Dollar, Token::Literal(String::from("var")),
        Token::Newline,
        Token::Literal(String::from("done")),
    ));
    assert_eq!(Err(IncompleteCmd("for", src(0,1,1), "in", src(8,1,9))), p.for_command());
}

#[test]
fn test_for_command_should_recognize_literals_and_names() {
    for for_tok in vec!(Token::Literal(String::from("for")), Token::Name(String::from("for"))) {
        for in_tok in vec!(Token::Literal(String::from("in")), Token::Name(String::from("in"))) {
            let mut p = make_parser_from_tokens(vec!(
                for_tok.clone(),
                Token::Whitespace(String::from(" ")),

                Token::Name(String::from("var")),
                Token::Whitespace(String::from(" ")),

                in_tok.clone(),
                Token::Whitespace(String::from(" ")),
                Token::Literal(String::from("one")),
                Token::Whitespace(String::from(" ")),
                Token::Literal(String::from("two")),
                Token::Whitespace(String::from(" ")),
                Token::Literal(String::from("three")),
                Token::Whitespace(String::from(" ")),
                Token::Newline,

                Token::Literal(String::from("do")),
                Token::Whitespace(String::from(" ")),

                Token::Literal(String::from("echo")),
                Token::Whitespace(String::from(" ")),
                Token::Dollar,
                Token::Name(String::from("var")),
                Token::Newline,

                Token::Literal(String::from("done")),
            ));
            p.for_command().unwrap();
        }
    }
}

#[test]
fn test_function_declaration_valid() {
    let correct = FunctionDef(
        String::from("foo"),
        Rc::new(CompoundCommand {
            kind: Brace(vec!(cmd_args("echo", &["body"]))),
            io: vec!(),
        })
    );

    assert_eq!(correct, make_parser("function foo()      { echo body; }").function_declaration().unwrap());
    assert_eq!(correct, make_parser("function foo ()     { echo body; }").function_declaration().unwrap());
    assert_eq!(correct, make_parser("function foo (    ) { echo body; }").function_declaration().unwrap());
    assert_eq!(correct, make_parser("function foo(    )  { echo body; }").function_declaration().unwrap());
    assert_eq!(correct, make_parser("function foo        { echo body; }").function_declaration().unwrap());
    assert_eq!(correct, make_parser("foo()               { echo body; }").function_declaration().unwrap());
    assert_eq!(correct, make_parser("foo ()              { echo body; }").function_declaration().unwrap());
    assert_eq!(correct, make_parser("foo (    )          { echo body; }").function_declaration().unwrap());
    assert_eq!(correct, make_parser("foo(    )           { echo body; }").function_declaration().unwrap());

    assert_eq!(correct, make_parser("function foo()     \n{ echo body; }").function_declaration().unwrap());
    assert_eq!(correct, make_parser("function foo ()    \n{ echo body; }").function_declaration().unwrap());
    assert_eq!(correct, make_parser("function foo (    )\n{ echo body; }").function_declaration().unwrap());
    assert_eq!(correct, make_parser("function foo(    ) \n{ echo body; }").function_declaration().unwrap());
    assert_eq!(correct, make_parser("function foo       \n{ echo body; }").function_declaration().unwrap());
    assert_eq!(correct, make_parser("foo()              \n{ echo body; }").function_declaration().unwrap());
    assert_eq!(correct, make_parser("foo ()             \n{ echo body; }").function_declaration().unwrap());
    assert_eq!(correct, make_parser("foo (    )         \n{ echo body; }").function_declaration().unwrap());
    assert_eq!(correct, make_parser("foo(    )          \n{ echo body; }").function_declaration().unwrap());
}

#[test]
fn test_function_declaration_valid_body_need_not_be_a_compound_command() {
    let src = vec!(
        ("function foo()      echo body;", src(20, 1, 21)),
        ("function foo ()     echo body;", src(20, 1, 21)),
        ("function foo (    ) echo body;", src(20, 1, 21)),
        ("function foo(    )  echo body;", src(20, 1, 21)),
        ("function foo        echo body;", src(20, 1, 21)),
        ("foo()               echo body;", src(20, 1, 21)),
        ("foo ()              echo body;", src(20, 1, 21)),
        ("foo (    )          echo body;", src(20, 1, 21)),
        ("foo(    )           echo body;", src(20, 1, 21)),

        ("function foo()     \necho body;", src(20, 2, 1)),
        ("function foo ()    \necho body;", src(20, 2, 1)),
        ("function foo (    )\necho body;", src(20, 2, 1)),
        ("function foo(    ) \necho body;", src(20, 2, 1)),
        ("function foo       \necho body;", src(20, 2, 1)),
        ("foo()              \necho body;", src(20, 2, 1)),
        ("foo ()             \necho body;", src(20, 2, 1)),
        ("foo (    )         \necho body;", src(20, 2, 1)),
        ("foo(    )          \necho body;", src(20, 2, 1)),
    );

    for (s, p) in src {
        let correct = Unexpected(Token::Name(String::from("echo")), p);
        match make_parser(s).function_declaration() {
            Ok(w) => panic!("Unexpectedly parsed the source \"{}\" as\n{:?}", s, w),
            Err(ref err) => if err != &correct {
                panic!("Expected the source \"{}\" to return the error `{:?}`, but got `{:?}`",
                       s, correct, err);
            },
        }
    }
}

#[test]
fn test_function_declaration_parens_can_be_subshell_if_function_keyword_present() {
    let correct = FunctionDef(
        String::from("foo"),
        Rc::new(CompoundCommand {
            kind: Subshell(vec!(cmd_args("echo", &["subshell"]))),
            io: vec!(),
        })
    );

    assert_eq!(correct, make_parser("function foo (echo subshell)").function_declaration().unwrap());
    assert_eq!(correct, make_parser("function foo() (echo subshell)").function_declaration().unwrap());
    assert_eq!(correct, make_parser("function foo () (echo subshell)").function_declaration().unwrap());
    assert_eq!(correct, make_parser("function foo\n(echo subshell)").function_declaration().unwrap());
}

#[test]
fn test_function_declaration_invalid_newline_in_declaration() {
    let mut p = make_parser("function\nname() { echo body; }");
    assert_eq!(Err(Unexpected(Token::Newline, src(8,1,9))), p.function_declaration());
    // If the function keyword is present the () are optional, and at this particular point
    // they become an empty subshell (which is invalid)
    let mut p = make_parser("function name\n() { echo body; }");
    assert_eq!(Err(Unexpected(Token::ParenClose, src(15,2,2))), p.function_declaration());
}

#[test]
fn test_function_declaration_invalid_missing_space_after_fn_keyword_and_no_parens() {
    let mut p = make_parser("functionname { echo body; }");
    assert_eq!(Err(Unexpected(Token::CurlyOpen, src(13,1,14))), p.function_declaration());
}

#[test]
fn test_function_declaration_invalid_missing_fn_keyword_and_parens() {
    let mut p = make_parser("name { echo body; }");
    assert_eq!(Err(Unexpected(Token::CurlyOpen, src(5,1,6))), p.function_declaration());
}

#[test]
fn test_function_declaration_invalid_missing_space_after_name_no_parens() {
    let mut p = make_parser("function name{ echo body; }");
    assert_eq!(Err(Unexpected(Token::CurlyOpen, src(13,1,14))), p.function_declaration());
    let mut p = make_parser("function name( echo body; )");
    assert_eq!(Err(Unexpected(Token::Name(String::from("echo")), src(15,1,16))), p.function_declaration());
}

#[test]
fn test_function_declaration_invalid_missing_name() {
    let mut p = make_parser("function { echo body; }");
    assert_eq!(Err(Unexpected(Token::CurlyOpen, src(9,1,10))), p.function_declaration());
    let mut p = make_parser("function () { echo body; }");
    assert_eq!(Err(Unexpected(Token::ParenOpen, src(9,1,10))), p.function_declaration());
    let mut p = make_parser("() { echo body; }");
    assert_eq!(Err(Unexpected(Token::ParenOpen, src(0,1,1))), p.function_declaration());
}

#[test]
fn test_function_declaration_invalid_missing_body() {
    let mut p = make_parser("function name");
    assert_eq!(Err(UnexpectedEOF), p.function_declaration());
    let mut p = make_parser("function name()");
    assert_eq!(Err(UnexpectedEOF), p.function_declaration());
    let mut p = make_parser("name()");
    assert_eq!(Err(UnexpectedEOF), p.function_declaration());
}

#[test]
fn test_function_declaration_invalid_quoted() {
    let cmds = [
        ("'function' name { echo body; }", Unexpected(Token::SingleQuote, src(0,1,1))),
        ("function 'name'() { echo body; }", Unexpected(Token::SingleQuote, src(9,1,10))),
        ("name'()' { echo body; }", Unexpected(Token::SingleQuote, src(4,1,5))),
        ("\"function\" name { echo body; }", Unexpected(Token::DoubleQuote, src(0,1,1))),
        ("function \"name\"() { echo body; }", Unexpected(Token::DoubleQuote, src(9,1,10))),
        ("name\"()\" { echo body; }", Unexpected(Token::DoubleQuote, src(4,1,5))),
    ];

    for &(c, ref e) in cmds.into_iter() {
        match make_parser(c).function_declaration() {
            Ok(result) => panic!("Unexpectedly parsed \"{}\" as\n{:#?}", c, result),
            Err(ref err) => if err != e {
                panic!("Expected the source \"{}\" to return the error `{:?}`, but got `{:?}`",
                       c, e, err);
            },
        }
    }
}

#[test]
fn test_function_declaration_invalid_fn_must_be_name() {
    let mut p = make_parser("function 123fn { echo body; }");
    assert_eq!(Err(BadIdent(String::from("123fn"), src(9, 1, 10))), p.function_declaration());
    let mut p = make_parser("function 123fn() { echo body; }");
    assert_eq!(Err(BadIdent(String::from("123fn"), src(9, 1, 10))), p.function_declaration());
    let mut p = make_parser("123fn() { echo body; }");
    assert_eq!(Err(BadIdent(String::from("123fn"), src(0, 1, 1))), p.function_declaration());
}

#[test]
fn test_function_declaration_invalid_fn_name_must_be_name_token() {
    let mut p = make_parser_from_tokens(vec!(
        Token::Literal(String::from("function")),
        Token::Whitespace(String::from(" ")),

        Token::Literal(String::from("fn_name")),
        Token::Whitespace(String::from(" ")),

        Token::ParenOpen, Token::ParenClose,
        Token::Whitespace(String::from(" ")),
        Token::CurlyOpen,
        Token::Whitespace(String::from(" ")),
        Token::Literal(String::from("echo")),
        Token::Whitespace(String::from(" ")),
        Token::Literal(String::from("fn body")),
        Token::Semi,
        Token::CurlyClose,
    ));
    assert_eq!(Err(BadIdent(String::from("fn_name"), src(9, 1, 10))), p.function_declaration());

    let mut p = make_parser_from_tokens(vec!(
        Token::Literal(String::from("function")),
        Token::Whitespace(String::from(" ")),

        Token::Name(String::from("fn_name")),
        Token::Whitespace(String::from(" ")),

        Token::ParenOpen, Token::ParenClose,
        Token::Whitespace(String::from(" ")),
        Token::CurlyOpen,
        Token::Whitespace(String::from(" ")),
        Token::Literal(String::from("echo")),
        Token::Whitespace(String::from(" ")),
        Token::Literal(String::from("fn body")),
        Token::Semi,
        Token::CurlyClose,
    ));
    p.function_declaration().unwrap();
}

#[test]
fn test_function_declaration_invalid_concat() {
    let mut p = make_parser_from_tokens(vec!(
        Token::Literal(String::from("func")), Token::Literal(String::from("tion")),
        Token::Whitespace(String::from(" ")),

        Token::Name(String::from("fn_name")),
        Token::Whitespace(String::from(" ")),

        Token::ParenOpen, Token::ParenClose,
        Token::Whitespace(String::from(" ")),
        Token::CurlyOpen,
        Token::Literal(String::from("echo")),
        Token::Whitespace(String::from(" ")),
        Token::Literal(String::from("fn body")),
        Token::Semi,
        Token::CurlyClose,
    ));
    assert_eq!(Err(BadIdent(String::from("func"), src(0, 1, 1))), p.function_declaration());
}

#[test]
fn test_function_declaration_should_recognize_literals_and_names_for_fn_keyword() {
    for fn_tok in vec!(Token::Literal(String::from("function")), Token::Name(String::from("function"))) {
        let mut p = make_parser_from_tokens(vec!(
            fn_tok,
            Token::Whitespace(String::from(" ")),

            Token::Name(String::from("fn_name")),
            Token::Whitespace(String::from(" ")),

            Token::ParenOpen, Token::ParenClose,
            Token::Whitespace(String::from(" ")),
            Token::CurlyOpen,
            Token::Whitespace(String::from(" ")),
            Token::Literal(String::from("echo")),
            Token::Whitespace(String::from(" ")),
            Token::Literal(String::from("fn body")),
            Token::Semi,
            Token::CurlyClose,
        ));
        p.function_declaration().unwrap();
    }
}

#[test]
fn test_case_command_valid() {
    let correct = CaseFragments {
        word: word("foo"),
        post_word_comments: vec!(),
        in_comment: None,
        arms: vec!(
            CaseArm {
                patterns: CasePatternFragments {
                    pre_pattern_comments: vec!(),
                    pattern_alternatives: vec!(word("hello"), word("goodbye")),
                    pattern_comment: None,
                },
                body: CommandGroup {
                    commands: vec!(cmd_args("echo", &["greeting"])),
                    trailing_comments: vec!(),
                },
                arm_comment: None,
            },
            CaseArm {
                patterns: CasePatternFragments {
                    pre_pattern_comments: vec!(),
                    pattern_alternatives: vec!(word("world")),
                    pattern_comment: None,
                },
                body: CommandGroup {
                    commands: vec!(cmd_args("echo", &["noun"])),
                    trailing_comments: vec!(),
                },
                arm_comment: None,
            },
        ),
        post_arms_comments: vec!(),
    };

    let cases = vec!(
        // `(` before pattern is optional
        "case foo in  hello | goodbye) echo greeting;;  world) echo noun;; esac",
        "case foo in (hello | goodbye) echo greeting;;  world) echo noun;; esac",
        "case foo in (hello | goodbye) echo greeting;; (world) echo noun;; esac",

        // Final `;;` is optional as long as last command is complete
        "case foo in hello | goodbye) echo greeting;; world) echo noun\nesac",
        "case foo in hello | goodbye) echo greeting;; world) echo noun; esac",
    );

    for src in cases {
        assert_eq!(correct, make_parser(src).case_command().unwrap());
    }
}

#[test]
fn test_case_command_valid_with_comments() {
    let correct = CaseFragments {
        word: word("foo"),
        post_word_comments: vec!(
            Newline(Some(String::from("#word_comment"))),
            Newline(Some(String::from("#post_word_a"))),
            Newline(None),
            Newline(Some(String::from("#post_word_b"))),
        ),
        in_comment: Some(Newline(Some(String::from("#in_comment")))),
        arms: vec!(
            CaseArm {
                patterns: CasePatternFragments {
                    pre_pattern_comments: vec!(
                        Newline(None),
                        Newline(Some(String::from("#pre_pat_a"))),
                    ),
                    pattern_alternatives: vec!(word("hello"), word("goodbye")),
                    pattern_comment: Some(Newline(Some(String::from("#pat_a")))),
                },
                body: CommandGroup {
                    commands: vec!(cmd_args("echo", &["greeting"])),
                    trailing_comments: vec!(
                        Newline(None),
                        Newline(Some(String::from("#post_body_a")))
                    ),
                },
                arm_comment: Some(Newline(Some(String::from("#arm_a")))),
            },
            CaseArm {
                patterns: CasePatternFragments {
                    pre_pattern_comments: vec!(
                        Newline(None),
                        Newline(Some(String::from("#pre_pat_b"))),
                    ),
                    pattern_alternatives: vec!(word("world")),
                    pattern_comment: Some(Newline(Some(String::from("#pat_b")))),
                },
                body: CommandGroup {
                    commands: vec!(cmd_args("echo", &["noun"])),
                    trailing_comments: vec!(),
                },
                arm_comment: Some(Newline(Some(String::from("#arm_b")))),
            },
        ),
        post_arms_comments: vec!(
            Newline(None),
            Newline(Some(String::from("#post_arms"))),
        ),
    };

    // Various newlines and comments allowed within the command
    let cmd =
        "case foo #word_comment
        #post_word_a

        #post_word_b
        in #in_comment

        #pre_pat_a
        (hello | goodbye) #pat_a

        #cmd_leading
        echo greeting #within_body

        #post_body_a
        ;; #arm_a

        #pre_pat_b
        world) #pat_b

        #cmd_leading
        echo noun
        ;; #arm_b

        #post_arms
        esac";

    assert_eq!(Ok(correct), make_parser(cmd).case_command());
}

#[test]
fn test_case_command_valid_with_comments_no_body() {
    let correct = CaseFragments {
        word: word("foo"),
        post_word_comments: vec!(
            Newline(Some(String::from("#word_comment"))),
            Newline(Some(String::from("#post_word_a"))),
            Newline(None),
            Newline(Some(String::from("#post_word_b"))),
        ),
        in_comment: Some(Newline(Some(String::from("#in_comment")))),
        arms: vec!(),
        post_arms_comments: vec!(
            Newline(None),
            Newline(Some(String::from("#post_arms"))),
        ),
    };

    // Various newlines and comments allowed within the command
    let cmd =
        "case foo #word_comment
        #post_word_a

        #post_word_b
        in #in_comment

        #post_arms
        esac #case_comment";

    assert_eq!(correct, make_parser(cmd).case_command().unwrap());
}

#[test]
fn test_case_command_word_need_not_be_simple_literal() {
    let mut p = make_parser("case 'foo'bar$$ in foo) echo foo;; esac");
    p.case_command().unwrap();
}

#[test]
fn test_case_command_valid_with_no_arms() {
    let mut p = make_parser("case foo in esac");
    p.case_command().unwrap();
}

#[test]
fn test_case_command_valid_branch_with_no_command() {
    let mut p = make_parser("case foo in pat)\nesac");
    p.case_command().unwrap();
    let mut p = make_parser("case foo in pat);;esac");
    p.case_command().unwrap();
}

#[test]
fn test_case_command_invalid_missing_keyword() {
    let mut p = make_parser("foo in foo) echo foo;; bar) echo bar;; esac");
    assert_eq!(Err(Unexpected(Token::Name(String::from("foo")), src(0, 1, 1))), p.case_command());
    let mut p = make_parser("case foo foo) echo foo;; bar) echo bar;; esac");
    assert_eq!(Err(IncompleteCmd("case", src(0,1,1), "in", src(9,1,10))), p.case_command());
    let mut p = make_parser("case foo in foo) echo foo;; bar) echo bar;;");
    assert_eq!(Err(IncompleteCmd("case", src(0,1,1), "esac", src(43,1,44))), p.case_command());
}

#[test]
fn test_case_command_invalid_missing_word() {
    let mut p = make_parser("case in foo) echo foo;; bar) echo bar;; esac");
    assert_eq!(Err(IncompleteCmd("case", src(0,1,1), "in", src(8,1,9))), p.case_command());
}

#[test]
fn test_case_command_invalid_quoted() {
    let cmds = [
        ("'case' foo in foo) echo foo;; bar) echo bar;; esac", Unexpected(Token::SingleQuote, src(0,1,1))),
        ("case foo 'in' foo) echo foo;; bar) echo bar;; esac", IncompleteCmd("case", src(0,1,1), "in", src(9,1,10))),
        ("case foo in foo) echo foo;; bar')' echo bar;; esac", Unexpected(Token::Name(String::from("echo")), src(35,1,36))),
        ("case foo in foo) echo foo;; bar) echo bar;; 'esac'", IncompleteCmd("case", src(0,1,1), "esac", src(50,1,51))),
        ("\"case\" foo in foo) echo foo;; bar) echo bar;; esac", Unexpected(Token::DoubleQuote, src(0,1,1))),
        ("case foo \"in\" foo) echo foo;; bar) echo bar;; esac", IncompleteCmd("case", src(0,1,1), "in", src(9,1,10))),
        ("case foo in foo) echo foo;; bar\")\" echo bar;; esac", Unexpected(Token::Name(String::from("echo")), src(35,1,36))),
        ("case foo in foo) echo foo;; bar) echo bar;; \"esac\"", IncompleteCmd("case", src(0,1,1), "esac", src(50,1,51))),
    ];

    for &(c, ref e) in cmds.into_iter() {
        match make_parser(c).case_command() {
            Ok(result) => panic!("Unexpectedly parsed \"{}\" as\n{:#?}", c, result),
            Err(ref err) => if err != e {
                panic!("Expected the source \"{}\" to return the error `{:?}`, but got `{:?}`",
                       c, e, err);
            },
        }
    }
}

#[test]
fn test_case_command_invalid_newline_after_case() {
    let mut p = make_parser("case\nfoo in foo) echo foo;; bar) echo bar;; esac");
    assert_eq!(Err(Unexpected(Token::Newline, src(4, 1, 5))), p.case_command());
}

#[test]
fn test_case_command_invalid_concat() {
    let mut p = make_parser_from_tokens(vec!(
        Token::Literal(String::from("ca")), Token::Literal(String::from("se")),
        Token::Whitespace(String::from(" ")),

        Token::Literal(String::from("foo")),
        Token::Literal(String::from("bar")),
        Token::Whitespace(String::from(" ")),

        Token::Literal(String::from("in")),
        Token::Literal(String::from("foo")),
        Token::ParenClose,
        Token::Newline,
        Token::Literal(String::from("echo")),
        Token::Whitespace(String::from(" ")),
        Token::Literal(String::from("foo")),
        Token::Newline,
        Token::Newline,
        Token::DSemi,

        Token::Literal(String::from("esac")),
    ));
    assert_eq!(Err(Unexpected(Token::Literal(String::from("ca")), src(0,1,1))), p.case_command());

    let mut p = make_parser_from_tokens(vec!(
        Token::Literal(String::from("case")),
        Token::Whitespace(String::from(" ")),

        Token::Literal(String::from("foo")),
        Token::Literal(String::from("bar")),
        Token::Whitespace(String::from(" ")),

        Token::Literal(String::from("i")), Token::Literal(String::from("n")),
        Token::Literal(String::from("foo")),
        Token::ParenClose,
        Token::Newline,
        Token::Literal(String::from("echo")),
        Token::Whitespace(String::from(" ")),
        Token::Literal(String::from("foo")),
        Token::Newline,
        Token::Newline,
        Token::DSemi,

        Token::Literal(String::from("esac")),
    ));
    assert_eq!(Err(IncompleteCmd("case", src(0,1,1), "in", src(12,1,13))), p.case_command());

    let mut p = make_parser_from_tokens(vec!(
        Token::Literal(String::from("case")),
        Token::Whitespace(String::from(" ")),

        Token::Literal(String::from("foo")),
        Token::Literal(String::from("bar")),
        Token::Whitespace(String::from(" ")),

        Token::Literal(String::from("in")),
        Token::Whitespace(String::from(" ")),
        Token::Literal(String::from("foo")),
        Token::ParenClose,
        Token::Newline,
        Token::Literal(String::from("echo")),
        Token::Whitespace(String::from(" ")),
        Token::Literal(String::from("foo")),
        Token::Newline,
        Token::Newline,
        Token::DSemi,

        Token::Literal(String::from("es")), Token::Literal(String::from("ac")),
    ));
    assert_eq!(Err(IncompleteCmd("case", src(0,1,1), "esac", src(36,4,7))), p.case_command());
}

#[test]
fn test_case_command_should_recognize_literals_and_names() {
    let case_str = String::from("case");
    let in_str   = String::from("in");
    let esac_str = String::from("esac");
    for case_tok in vec!(Token::Literal(case_str.clone()), Token::Name(case_str.clone())) {
        for in_tok in vec!(Token::Literal(in_str.clone()), Token::Name(in_str.clone())) {
            for esac_tok in vec!(Token::Literal(esac_str.clone()), Token::Name(esac_str.clone())) {
                let mut p = make_parser_from_tokens(vec!(
                    case_tok.clone(),
                    Token::Whitespace(String::from(" ")),

                    Token::Literal(String::from("foo")),
                    Token::Literal(String::from("bar")),

                    Token::Whitespace(String::from(" ")),
                    in_tok.clone(),
                    Token::Whitespace(String::from(" ")),

                    Token::Literal(String::from("foo")),
                    Token::ParenClose,
                    Token::Newline,
                    Token::Literal(String::from("echo")),
                    Token::Whitespace(String::from(" ")),
                    Token::Literal(String::from("foo")),
                    Token::Newline,
                    Token::Newline,
                    Token::DSemi,

                    esac_tok
                ));
                p.case_command().unwrap();
            }
        }
    }
}

#[test]
fn test_compound_command_delegates_valid_commands_brace() {
    let correct = CompoundCommand {
        kind: Brace(vec!(cmd("foo"))),
        io: vec!(),
    };
    assert_eq!(correct, make_parser("{ foo; }").compound_command().unwrap());
}

#[test]
fn test_compound_command_delegates_valid_commands_subshell() {
    let commands = [
        "(foo)",
        "( foo)",
    ];

    let correct = CompoundCommand {
        kind: Subshell(vec!(cmd("foo"))),
        io: vec!(),
    };

    for cmd in &commands {
        match make_parser(cmd).compound_command() {
            Ok(ref result) if result == &correct => {},
            Ok(result) => panic!("Parsed \"{}\" as an unexpected command type:\n{:#?}", cmd, result),
            Err(err) => panic!("Failed to parse \"{}\": {}", cmd, err),
        }
    }
}

#[test]
fn test_compound_command_delegates_valid_commands_while() {
    let correct = CompoundCommand {
        kind: While(GuardBodyPair {
            guard: vec!(cmd("guard")),
            body: vec!(cmd("foo")),
        }),
        io: vec!(),
    };
    assert_eq!(correct, make_parser("while guard; do foo; done").compound_command().unwrap());
}

#[test]
fn test_compound_command_delegates_valid_commands_until() {
    let correct = CompoundCommand {
        kind: Until(GuardBodyPair {
            guard: vec!(cmd("guard")),
            body: vec!(cmd("foo")),
        }),
        io: vec!(),
    };
    assert_eq!(correct, make_parser("until guard; do foo; done").compound_command().unwrap());
}

#[test]
fn test_compound_command_delegates_valid_commands_for() {
    let correct = CompoundCommand {
        kind: For {
            var: String::from("var"),
            words: Some(vec!()),
            body: vec!(cmd("foo")),
        },
        io: vec!(),
    };
    assert_eq!(correct, make_parser("for var in; do foo; done").compound_command().unwrap());
}

#[test]
fn test_compound_command_delegates_valid_commands_if() {
    let correct = CompoundCommand {
        kind: If {
            conditionals: vec!(GuardBodyPair {
                guard: vec!(cmd("guard")),
                body: vec!(cmd("body")),
            }),
            else_branch: None,
        },
        io: vec!(),
    };
    assert_eq!(correct, make_parser("if guard; then body; fi").compound_command().unwrap());
}

#[test]
fn test_compound_command_delegates_valid_commands_case() {
    let correct = CompoundCommand {
        kind: Case {
            word: word("foo"),
            arms: vec!(),
        },
        io: vec!(),
    };
    assert_eq!(correct, make_parser("case foo in esac").compound_command().unwrap());
}

#[test]
fn test_compound_command_errors_on_quoted_commands() {
    let cases = [
        // { is a reserved word, thus concatenating it essentially "quotes" it
        // `compound_command` doesn't know or care enough to specify that "foo" is
        // the problematic token instead of {, however, callers who are smart enough
        // to expect a brace command would be aware themselves that no such valid
        // command actually exists. TL;DR: it's okay for `compound_command` to blame {
        ("{foo; }",                      Unexpected(Token::CurlyOpen,   src(0,1,1))),
        ("'{' foo; }",                   Unexpected(Token::SingleQuote, src(0,1,1))),
        ("'(' foo; )",                   Unexpected(Token::SingleQuote, src(0,1,1))),
        ("'while' guard do foo; done",   Unexpected(Token::SingleQuote, src(0,1,1))),
        ("'until' guard do foo; done",   Unexpected(Token::SingleQuote, src(0,1,1))),
        ("'if' guard; then body; fi",    Unexpected(Token::SingleQuote, src(0,1,1))),
        ("'for' var in; do foo; done",   Unexpected(Token::SingleQuote, src(0,1,1))),
        ("'case' foo in esac",           Unexpected(Token::SingleQuote, src(0,1,1))),
        ("\"{\" foo; }",                 Unexpected(Token::DoubleQuote, src(0,1,1))),
        ("\"(\" foo; )",                 Unexpected(Token::DoubleQuote, src(0,1,1))),
        ("\"while\" guard do foo; done", Unexpected(Token::DoubleQuote, src(0,1,1))),
        ("\"until\" guard do foo; done", Unexpected(Token::DoubleQuote, src(0,1,1))),
        ("\"if\" guard; then body; fi",  Unexpected(Token::DoubleQuote, src(0,1,1))),
        ("\"for\" var in; do foo; done", Unexpected(Token::DoubleQuote, src(0,1,1))),
        ("\"case\" foo in esac",         Unexpected(Token::DoubleQuote, src(0,1,1))),
    ];

    for &(src, ref e) in &cases {
        match make_parser(src).compound_command() {
            Ok(result) =>
                panic!("Parse::compound_command unexpectedly succeeded parsing \"{}\" with result:\n{:#?}",
                       src, result),
            Err(ref err) => if err != e {
                panic!("Expected the source \"{}\" to return the error `{:?}`, but got `{:?}`",
                       src, e, err);
            },
        }
    }
}

#[test]
fn test_compound_command_captures_redirections_after_command() {
    let cases = [
        "{ foo; } 1>>out <& 2 2>&-",
        "( foo; ) 1>>out <& 2 2>&-",
        "while guard; do foo; done 1>>out <& 2 2>&-",
        "until guard; do foo; done 1>>out <& 2 2>&-",
        "if guard; then body; fi 1>>out <& 2 2>&-",
        "for var in; do foo; done 1>>out <& 2 2>&-",
        "case foo in esac 1>>out <& 2 2>&-",
    ];

    for cmd in &cases {
        match make_parser(cmd).compound_command() {
            Ok(CompoundCommand { io, .. }) => assert_eq!(io, vec!(
                Redirect::Append(Some(1), word("out")),
                Redirect::DupRead(None, word("2")),
                Redirect::DupWrite(Some(2), word("-")),
            )),

            Err(err) => panic!("Failed to parse \"{}\": {}", cmd, err),
        }
    }
}

#[test]
fn test_compound_command_should_delegate_literals_and_names_loop() {
    for kw in vec!(
        Token::Literal(String::from("while")),
        Token::Name(String::from("while")),
        Token::Literal(String::from("until")),
        Token::Name(String::from("until")))
    {
        let mut p = make_parser_from_tokens(vec!(
            kw,
            Token::Newline,
            Token::Literal(String::from("guard")),
            Token::Newline,
            Token::Literal(String::from("do")),
            Token::Newline,
            Token::Literal(String::from("foo")),
            Token::Newline,
            Token::Literal(String::from("done")),
        ));
        p.compound_command().unwrap();
    }
}

#[test]
fn test_compound_command_should_delegate_literals_and_names_if() {
    for if_tok in vec!(Token::Literal(String::from("if")), Token::Name(String::from("if"))) {
        for then_tok in vec!(Token::Literal(String::from("then")), Token::Name(String::from("then"))) {
            for elif_tok in vec!(Token::Literal(String::from("elif")), Token::Name(String::from("elif"))) {
                for else_tok in vec!(Token::Literal(String::from("else")), Token::Name(String::from("else"))) {
                    for fi_tok in vec!(Token::Literal(String::from("fi")), Token::Name(String::from("fi"))) {
                        let mut p = make_parser_from_tokens(vec!(
                                if_tok.clone(),
                                Token::Whitespace(String::from(" ")),

                                Token::Literal(String::from("guard1")),
                                Token::Newline,

                                then_tok.clone(),
                                Token::Newline,
                                Token::Literal(String::from("body1")),

                                elif_tok.clone(),
                                Token::Whitespace(String::from(" ")),

                                Token::Literal(String::from("guard2")),
                                Token::Newline,
                                then_tok.clone(),
                                Token::Whitespace(String::from(" ")),
                                Token::Literal(String::from("body2")),

                                else_tok.clone(),
                                Token::Whitespace(String::from(" ")),

                                Token::Whitespace(String::from(" ")),
                                Token::Literal(String::from("else part")),
                                Token::Newline,

                                fi_tok,
                        ));
                        p.compound_command().unwrap();
                    }
                }
            }
        }
    }
}

#[test]
fn test_compound_command_should_delegate_literals_and_names_for() {
    for for_tok in vec!(Token::Literal(String::from("for")), Token::Name(String::from("for"))) {
        for in_tok in vec!(Token::Literal(String::from("in")), Token::Name(String::from("in"))) {
            let mut p = make_parser_from_tokens(vec!(
                for_tok.clone(),
                Token::Whitespace(String::from(" ")),

                Token::Name(String::from("var")),
                Token::Whitespace(String::from(" ")),

                in_tok.clone(),
                Token::Whitespace(String::from(" ")),
                Token::Literal(String::from("one")),
                Token::Whitespace(String::from(" ")),
                Token::Literal(String::from("two")),
                Token::Whitespace(String::from(" ")),
                Token::Literal(String::from("three")),
                Token::Whitespace(String::from(" ")),
                Token::Newline,

                Token::Literal(String::from("do")),
                Token::Whitespace(String::from(" ")),

                Token::Literal(String::from("echo")),
                Token::Whitespace(String::from(" ")),
                Token::Dollar,
                Token::Name(String::from("var")),
                Token::Newline,

                Token::Literal(String::from("done")),
            ));
            p.compound_command().unwrap();
        }
    }
}

#[test]
fn test_compound_command_should_delegate_literals_and_names_case() {
    let case_str = String::from("case");
    let in_str   = String::from("in");
    let esac_str = String::from("esac");
    for case_tok in vec!(Token::Literal(case_str.clone()), Token::Name(case_str.clone())) {
        for in_tok in vec!(Token::Literal(in_str.clone()), Token::Name(in_str.clone())) {
            for esac_tok in vec!(Token::Literal(esac_str.clone()), Token::Name(esac_str.clone())) {
                let mut p = make_parser_from_tokens(vec!(
                    case_tok.clone(),
                    Token::Whitespace(String::from(" ")),

                    Token::Literal(String::from("foo")),
                    Token::Literal(String::from("bar")),

                    Token::Whitespace(String::from(" ")),
                    in_tok.clone(),
                    Token::Whitespace(String::from(" ")),

                    Token::Literal(String::from("foo")),
                    Token::ParenClose,
                    Token::Newline,
                    Token::Literal(String::from("echo")),
                    Token::Whitespace(String::from(" ")),
                    Token::Literal(String::from("foo")),
                    Token::Newline,
                    Token::Newline,
                    Token::DSemi,

                    esac_tok
                ));
                p.compound_command().unwrap();
            }
        }
    }
}

#[test]
fn test_command_delegates_valid_commands_brace() {
    let correct = Compound(Box::new(CompoundCommand {
        kind: Brace(vec!(cmd("foo"))),
        io: vec!(),
    }));
    assert_eq!(correct, make_parser("{ foo; }").command().unwrap());
}

#[test]
fn test_command_delegates_valid_commands_subshell() {
    let commands = [
        "(foo)",
        "( foo)",
    ];

    let correct = Compound(Box::new(CompoundCommand {
        kind: Subshell(vec!(cmd("foo"))),
        io: vec!(),
    }));

    for cmd in &commands {
        match make_parser(cmd).command() {
            Ok(ref result) if result == &correct => {},
            Ok(result) => panic!("Parsed \"{}\" as an unexpected command type:\n{:#?}", cmd, result),
            Err(err) => panic!("Failed to parse \"{}\": {}", cmd, err),
        }
    }
}

#[test]
fn test_command_delegates_valid_commands_while() {
    let correct = Compound(Box::new(CompoundCommand {
        kind: While(GuardBodyPair {
            guard: vec!(cmd("guard")),
            body: vec!(cmd("foo")),
        }),
        io: vec!(),
    }));
    assert_eq!(correct, make_parser("while guard; do foo; done").command().unwrap());
}

#[test]
fn test_command_delegates_valid_commands_until() {
    let correct = Compound(Box::new(CompoundCommand {
        kind: Until(GuardBodyPair {
            guard: vec!(cmd("guard")),
            body: vec!(cmd("foo")),
        }),
        io: vec!(),
    }));
    assert_eq!(correct, make_parser("until guard; do foo; done").command().unwrap());
}

#[test]
fn test_command_delegates_valid_commands_for() {
    let correct = Compound(Box::new(CompoundCommand {
        kind: For {
            var: String::from("var"),
            words: Some(vec!()),
            body: vec!(cmd("foo")),
        },
        io: vec!(),
    }));
    assert_eq!(correct, make_parser("for var in; do foo; done").command().unwrap());
}

#[test]
fn test_command_delegates_valid_commands_if() {
    let correct = Compound(Box::new(CompoundCommand {
        kind: If {
            conditionals: vec!(GuardBodyPair {
                guard: vec!(cmd("guard")),
                body: vec!(cmd("body")),
            }),
            else_branch: None,
        },
        io: vec!(),
    }));
    assert_eq!(correct, make_parser("if guard; then body; fi").command().unwrap());
}

#[test]
fn test_command_delegates_valid_commands_case() {
    let correct = Compound(Box::new(CompoundCommand {
        kind: Case {
            word: word("foo"),
            arms: vec!(),
        },
        io: vec!(),
    }));
    assert_eq!(correct, make_parser("case foo in esac").command().unwrap());
}

#[test]
fn test_command_delegates_valid_simple_commands() {
    let correct = Simple(cmd_args_simple("echo", &["foo", "bar"]));
    assert_eq!(correct, make_parser("echo foo bar").command().unwrap());
}

#[test]
fn test_command_delegates_valid_commands_function() {
    let commands = [
        "function foo()      { echo body; }",
        "function foo ()     { echo body; }",
        "function foo (    ) { echo body; }",
        "function foo(    )  { echo body; }",
        "function foo        { echo body; }",
        "foo()               { echo body; }",
        "foo ()              { echo body; }",
        "foo (    )          { echo body; }",
        "foo(    )           { echo body; }",
    ];

    let correct = FunctionDef(String::from("foo"), Rc::new(CompoundCommand {
        kind: Brace(vec!(cmd_args("echo", &["body"]))),
        io: vec!(),
    }));

    for cmd in &commands {
        match make_parser(cmd).command() {
            Ok(ref result) if result == &correct => {},
            Ok(result) => panic!("Parsed \"{}\" as an unexpected command type:\n{:#?}", cmd, result),
            Err(err) => panic!("Failed to parse \"{}\": {}", cmd, err),
        }
    }
}

#[test]
fn test_command_parses_quoted_compound_commands_as_simple_commands() {
    let cases = [
        "{foo; }", // { is a reserved word, thus concatenating it essentially "quotes" it
        "'{' foo; }",
        "'(' foo; )",
        "'while' guard do foo; done",
        "'until' guard do foo; done",
        "'if' guard; then body; fi",
        "'for' var in; do echo $var; done",
        "'function' name { echo body; }",
        "name'()' { echo body; }",
        "123fn() { echo body; }",
        "'case' foo in esac",
        "\"{\" foo; }",
        "\"(\" foo; )",
        "\"while\" guard do foo; done",
        "\"until\" guard do foo; done",
        "\"if\" guard; then body; fi",
        "\"for\" var in; do echo $var; done",
        "\"function\" name { echo body; }",
        "name\"()\" { echo body; }",
        "\"case\" foo in esac",
    ];

    for cmd in &cases {
        match make_parser(cmd).command() {
            Ok(Simple(_)) => {},
            Ok(result) =>
                panic!("Parse::command unexpectedly parsed \"{}\" as a non-simple command:\n{:#?}", cmd, result),
            Err(err) => panic!("Parse::command failed to parse \"{}\": {}", cmd, err),
        }
    }
}

#[test]
fn test_command_should_delegate_literals_and_names_loop_while() {
    for kw in vec!(
        Token::Literal(String::from("while")),
        Token::Name(String::from("while"))
    ) {
        let mut p = make_parser_from_tokens(vec!(
            kw,
            Token::Newline,
            Token::Literal(String::from("guard")),
            Token::Newline,
            Token::Literal(String::from("do")),
            Token::Newline,
            Token::Literal(String::from("foo")),
            Token::Newline,
            Token::Literal(String::from("done")),
        ));

        let cmd = p.command().unwrap();
        if let Compound(ref compound_cmd) = cmd {
            if let While(..) = compound_cmd.kind {
                continue;
            }
        }

        panic!("Parsed an unexpected command:\n{:#?}", cmd)
    }
}

#[test]
fn test_command_should_delegate_literals_and_names_loop_until() {
    for kw in vec!(
        Token::Literal(String::from("until")),
        Token::Name(String::from("until"))
    ) {
        let mut p = make_parser_from_tokens(vec!(
            kw,
            Token::Newline,
            Token::Literal(String::from("guard")),
            Token::Newline,
            Token::Literal(String::from("do")),
            Token::Newline,
            Token::Literal(String::from("foo")),
            Token::Newline,
            Token::Literal(String::from("done")),
        ));

        let cmd = p.command().unwrap();
        if let Compound(ref compound_cmd) = cmd {
            if let Until(..) = compound_cmd.kind {
                continue;
            }
        }

        panic!("Parsed an unexpected command:\n{:#?}", cmd)
    }
}

#[test]
fn test_command_should_delegate_literals_and_names_if() {
    for if_tok in vec!(Token::Literal(String::from("if")), Token::Name(String::from("if"))) {
        for then_tok in vec!(Token::Literal(String::from("then")), Token::Name(String::from("then"))) {
            for elif_tok in vec!(Token::Literal(String::from("elif")), Token::Name(String::from("elif"))) {
                for else_tok in vec!(Token::Literal(String::from("else")), Token::Name(String::from("else"))) {
                    for fi_tok in vec!(Token::Literal(String::from("fi")), Token::Name(String::from("fi"))) {
                        let mut p = make_parser_from_tokens(vec!(
                                if_tok.clone(),
                                Token::Whitespace(String::from(" ")),

                                Token::Literal(String::from("guard1")),
                                Token::Newline,

                                then_tok.clone(),
                                Token::Newline,
                                Token::Literal(String::from("body1")),

                                elif_tok.clone(),
                                Token::Whitespace(String::from(" ")),

                                Token::Literal(String::from("guard2")),
                                Token::Newline,
                                then_tok.clone(),
                                Token::Whitespace(String::from(" ")),
                                Token::Literal(String::from("body2")),

                                else_tok.clone(),
                                Token::Whitespace(String::from(" ")),

                                Token::Whitespace(String::from(" ")),
                                Token::Literal(String::from("else part")),
                                Token::Newline,

                                fi_tok,
                        ));

                        let cmd = p.command().unwrap();
                        if let Compound(ref compound_cmd) = cmd {
                            if let If { .. } = compound_cmd.kind {
                                continue;
                            }
                        }

                        panic!("Parsed an unexpected command:\n{:#?}", cmd)
                    }
                }
            }
        }
    }
}

#[test]
fn test_command_should_delegate_literals_and_names_for() {
    for for_tok in vec!(Token::Literal(String::from("for")), Token::Name(String::from("for"))) {
        for in_tok in vec!(Token::Literal(String::from("in")), Token::Name(String::from("in"))) {
            let mut p = make_parser_from_tokens(vec!(
                for_tok.clone(),
                Token::Whitespace(String::from(" ")),

                Token::Name(String::from("var")),
                Token::Whitespace(String::from(" ")),

                in_tok.clone(),
                Token::Whitespace(String::from(" ")),
                Token::Literal(String::from("one")),
                Token::Whitespace(String::from(" ")),
                Token::Literal(String::from("two")),
                Token::Whitespace(String::from(" ")),
                Token::Literal(String::from("three")),
                Token::Whitespace(String::from(" ")),
                Token::Newline,

                Token::Literal(String::from("do")),
                Token::Whitespace(String::from(" ")),

                Token::Literal(String::from("echo")),
                Token::Whitespace(String::from(" ")),
                Token::Dollar,
                Token::Name(String::from("var")),
                Token::Newline,

                Token::Literal(String::from("done")),
            ));

            let cmd = p.command().unwrap();
            if let Compound(ref compound_cmd) = cmd {
                if let For { .. } = compound_cmd.kind {
                    continue;
                }
            }

            panic!("Parsed an unexpected command:\n{:#?}", cmd)
        }
    }
}

#[test]
fn test_command_should_delegate_literals_and_names_case() {
    let case_str = String::from("case");
    let in_str   = String::from("in");
    let esac_str = String::from("esac");
    for case_tok in vec!(Token::Literal(case_str.clone()), Token::Name(case_str.clone())) {
        for in_tok in vec!(Token::Literal(in_str.clone()), Token::Name(in_str.clone())) {
            for esac_tok in vec!(Token::Literal(esac_str.clone()), Token::Name(esac_str.clone())) {
                let mut p = make_parser_from_tokens(vec!(
                    case_tok.clone(),
                    Token::Whitespace(String::from(" ")),

                    Token::Literal(String::from("foo")),
                    Token::Literal(String::from("bar")),

                    Token::Whitespace(String::from(" ")),
                    in_tok.clone(),
                    Token::Whitespace(String::from(" ")),

                    Token::Literal(String::from("foo")),
                    Token::ParenClose,
                    Token::Newline,
                    Token::Literal(String::from("echo")),
                    Token::Whitespace(String::from(" ")),
                    Token::Literal(String::from("foo")),
                    Token::Newline,
                    Token::Newline,
                    Token::DSemi,

                    esac_tok
                ));

                let cmd = p.command().unwrap();
                if let Compound(ref compound_cmd) = cmd {
                    if let Case { .. } = compound_cmd.kind {
                        continue;
                    }
                }

                panic!("Parsed an unexpected command:\n{:#?}", cmd)
            }
        }
    }
}

#[test]
fn test_command_should_delegate_literals_and_names_for_function_declaration() {
    for fn_tok in vec!(Token::Literal(String::from("function")), Token::Name(String::from("function"))) {
        let mut p = make_parser_from_tokens(vec!(
            fn_tok,
            Token::Whitespace(String::from(" ")),

            Token::Name(String::from("fn_name")),
            Token::Whitespace(String::from(" ")),

            Token::ParenOpen, Token::ParenClose,
            Token::Whitespace(String::from(" ")),
            Token::CurlyOpen,
            Token::Whitespace(String::from(" ")),
            Token::Literal(String::from("echo")),
            Token::Whitespace(String::from(" ")),
            Token::Literal(String::from("fn body")),
            Token::Semi,
            Token::CurlyClose,
        ));
        match p.command() {
            Ok(FunctionDef(..)) => {},
            Ok(result) => panic!("Parsed an unexpected command type:\n{:#?}", result),
            Err(err) => panic!("Failed to parse command: {}", err),
        }
    }
}

#[test]
fn test_command_do_not_delegate_functions_only_if_fn_name_is_a_literal_token() {
    let mut p = make_parser_from_tokens(vec!(
        Token::Literal(String::from("fn_name")),
        Token::Whitespace(String::from(" ")),

        Token::ParenOpen, Token::ParenClose,
        Token::Whitespace(String::from(" ")),
        Token::CurlyOpen,
        Token::Literal(String::from("echo")),
        Token::Whitespace(String::from(" ")),
        Token::Literal(String::from("fn body")),
        Token::Semi,
        Token::CurlyClose,
    ));
    match p.command() {
        Ok(Simple(..)) => {},
        Ok(result) => panic!("Parsed an unexpected command type:\n{:#?}", result),
        Err(err) => panic!("Failed to parse command: {}", err),
    }
}

#[test]
fn test_command_delegate_functions_only_if_fn_name_is_a_name_token() {
    let mut p = make_parser_from_tokens(vec!(
        Token::Name(String::from("fn_name")),
        Token::Whitespace(String::from(" ")),

        Token::ParenOpen, Token::ParenClose,
        Token::Whitespace(String::from(" ")),
        Token::CurlyOpen,
        Token::Whitespace(String::from(" ")),
        Token::Literal(String::from("echo")),
        Token::Whitespace(String::from(" ")),
        Token::Literal(String::from("fn body")),
        Token::Semi,
        Token::CurlyClose,
    ));
    match p.command() {
        Ok(FunctionDef(..)) => {},
        Ok(result) => panic!("Parsed an unexpected command type:\n{:#?}", result),
        Err(err) => panic!("Failed to parse command: {}", err),
    }
}

#[test]
fn test_word_single_quote_valid() {
    let correct = single_quoted("abc&&||\n\n#comment\nabc");
    assert_eq!(Some(correct), make_parser("'abc&&||\n\n#comment\nabc'").word().unwrap());
}

#[test]
fn test_word_single_quote_valid_slash_remains_literal() {
    let correct = single_quoted("\\\n");
    assert_eq!(Some(correct), make_parser("'\\\n'").word().unwrap());
}

#[test]
fn test_word_single_quote_valid_does_not_quote_single_quotes() {
    let correct = single_quoted("hello \\");
    assert_eq!(Some(correct), make_parser("'hello \\'").word().unwrap());
}

#[test]
fn test_word_single_quote_invalid_missing_close_quote() {
    assert_eq!(Err(Unmatched(Token::SingleQuote, src(0, 1, 1))), make_parser("'hello").word());
}

#[test]
fn test_word_double_quote_valid() {
    let correct = TopLevelWord(Single(Word::DoubleQuoted(vec!(Literal(String::from("abc&&||\n\n#comment\nabc"))))));
    assert_eq!(Some(correct), make_parser("\"abc&&||\n\n#comment\nabc\"").word().unwrap());
}

#[test]
fn test_word_double_quote_valid_recognizes_parameters() {
    let correct = TopLevelWord(Single(Word::DoubleQuoted(vec!(
        Literal(String::from("test asdf")),
        Param(Parameter::Var(String::from("foo"))),
        Literal(String::from(" $")),
    ))));

    assert_eq!(Some(correct), make_parser("\"test asdf$foo $\"").word().unwrap());
}

#[test]
fn test_word_double_quote_valid_recognizes_backticks() {
    let correct = TopLevelWord(Single(Word::DoubleQuoted(vec!(
        Literal(String::from("test asdf ")),
        Subst(Box::new(ParameterSubstitution::Command(vec!(cmd("foo"))))),
    ))));

    assert_eq!(Some(correct), make_parser("\"test asdf `foo`\"").word().unwrap());
}

#[test]
fn test_word_double_quote_valid_slash_escapes_dollar() {
    let correct = TopLevelWord(Single(Word::DoubleQuoted(vec!(
        Literal(String::from("test")),
        Escaped(String::from("$")),
        Literal(String::from("foo ")),
        Param(Parameter::At),
    ))));

    assert_eq!(Some(correct), make_parser("\"test\\$foo $@\"").word().unwrap());
}

#[test]
fn test_word_double_quote_valid_slash_escapes_backtick() {
    let correct = TopLevelWord(Single(Word::DoubleQuoted(vec!(
        Literal(String::from("test")),
        Escaped(String::from("`")),
        Literal(String::from(" ")),
        Param(Parameter::Star),
    ))));

    assert_eq!(Some(correct), make_parser("\"test\\` $*\"").word().unwrap());
}

#[test]
fn test_word_double_quote_valid_slash_escapes_double_quote() {
    let correct = TopLevelWord(Single(Word::DoubleQuoted(vec!(
        Literal(String::from("test")),
        Escaped(String::from("\"")),
        Literal(String::from(" ")),
        Param(Parameter::Pound),
    ))));

    assert_eq!(Some(correct), make_parser("\"test\\\" $#\"").word().unwrap());
}

#[test]
fn test_word_double_quote_valid_slash_escapes_newline() {
    let correct = TopLevelWord(Single(Word::DoubleQuoted(vec!(
        Literal(String::from("test")),
        Escaped(String::from("\n")),
        Literal(String::from(" ")),
        Param(Parameter::Question),
        Literal(String::from("\n")),
    ))));

    assert_eq!(Some(correct), make_parser("\"test\\\n $?\n\"").word().unwrap());
}

#[test]
fn test_word_double_quote_valid_slash_escapes_slash() {
    let correct = TopLevelWord(Single(Word::DoubleQuoted(vec!(
        Literal(String::from("test")),
        Escaped(String::from("\\")),
        Literal(String::from(" ")),
        Param(Parameter::Positional(0)),
    ))));

    assert_eq!(Some(correct), make_parser("\"test\\\\ $0\"").word().unwrap());
}

#[test]
fn test_word_double_quote_valid_slash_remains_literal_in_general_case() {
    let correct = TopLevelWord(Single(Word::DoubleQuoted(vec!(
        Literal(String::from("t\\est ")),
        Param(Parameter::Dollar),
    ))));

    assert_eq!(Some(correct), make_parser("\"t\\est $$\"").word().unwrap());
}

#[test]
fn test_word_double_quote_slash_invalid_missing_close_quote() {
    assert_eq!(Err(Unmatched(Token::DoubleQuote, src(0, 1, 1))), make_parser("\"hello").word());
    assert_eq!(Err(Unmatched(Token::DoubleQuote, src(0, 1, 1))), make_parser("\"hello\\\"").word());
}

#[test]
fn test_word_delegate_parameters() {
    let params = [
        "$@",
        "$*",
        "$#",
        "$?",
        "$-",
        "$$",
        "$!",
        "$3",
        "${@}",
        "${*}",
        "${#}",
        "${?}",
        "${-}",
        "${$}",
        "${!}",
        "${foo}",
        "${3}",
        "${1000}",
    ];

    for p in params.into_iter() {
        match make_parser(p).word() {
            Ok(Some(TopLevelWord(Single(Word::Simple(w))))) => if let Param(_) = w {} else {
                panic!("Unexpectedly parsed \"{}\" as a non-parameter word:\n{:#?}", p, w);
            },
            Ok(Some(w)) => panic!("Unexpectedly parsed \"{}\" as a non-parameter word:\n{:#?}", p, w),
            Ok(None) => panic!("Did not parse \"{}\" as a parameter", p),
            Err(e) => panic!("Did not parse \"{}\" as a parameter: {}", p, e),
        }
    }
}

#[test]
fn test_word_literal_dollar_if_not_param() {
    let correct = word("$%asdf");
    assert_eq!(correct, make_parser("$%asdf").word().unwrap().unwrap());
}

#[test]
fn test_word_does_not_capture_comments() {
    assert_eq!(Ok(None), make_parser("#comment\n").word());
    assert_eq!(Ok(None), make_parser("  #comment\n").word());
    let mut p = make_parser("word   #comment\n");
    p.word().unwrap().unwrap();
    assert_eq!(Ok(None), p.word());
}

#[test]
fn test_word_pound_in_middle_is_not_comment() {
    let correct = word("abc#def");
    assert_eq!(Ok(Some(correct)), make_parser("abc#def\n").word());
}

#[test]
fn test_word_tokens_which_become_literal_words() {
    let words = [
        "{",
        "}",
        "!",
        "name",
        "1notname",
    ];

    for w in words.into_iter() {
        match make_parser(w).word() {
            Ok(Some(res)) => {
                let correct = word(*w);
                if correct != res {
                    panic!("Unexpectedly parsed \"{}\": expected:\n{:#?}\ngot:\n{:#?}", w, correct, res);
                }
            },
            Ok(None) => panic!("Did not parse \"{}\" as a word", w),
            Err(e) => panic!("Did not parse \"{}\" as a word: {}", w, e),
        }
    }
}

#[test]
fn test_word_concatenation_works() {
    let correct = TopLevelWord(Concat(vec!(
        lit("foo=bar"),
        Word::DoubleQuoted(vec!(Literal(String::from("double")))),
        Word::SingleQuoted(String::from("single")),
    )));

    assert_eq!(Ok(Some(correct)), make_parser("foo=bar\"double\"'single'").word());
}

#[test]
fn test_word_special_words_recognized_as_such() {
    assert_eq!(Ok(Some(TopLevelWord(Single(Word::Simple(Star))))),        make_parser("*").word());
    assert_eq!(Ok(Some(TopLevelWord(Single(Word::Simple(Question))))),    make_parser("?").word());
    assert_eq!(Ok(Some(TopLevelWord(Single(Word::Simple(Tilde))))),       make_parser("~").word());
    assert_eq!(Ok(Some(TopLevelWord(Single(Word::Simple(SquareOpen))))),  make_parser("[").word());
    assert_eq!(Ok(Some(TopLevelWord(Single(Word::Simple(SquareClose))))), make_parser("]").word());
    assert_eq!(Ok(Some(TopLevelWord(Single(Word::Simple(Colon))))),       make_parser(":").word());
}

#[test]
fn test_word_backslash_makes_things_literal() {
    let lit = [
        "a",
        "&",
        ";",
        "(",
        "*",
        "?",
        "$",
    ];

    for l in lit.into_iter() {
        let src = format!("\\{}", l);
        match make_parser(&src).word() {
            Ok(Some(res)) => {
                let correct = word_escaped(l);
                if correct != res {
                    panic!("Unexpectedly parsed \"{}\": expected:\n{:#?}\ngot:\n{:#?}", src, correct, res);
                }
            },
            Ok(None) => panic!("Did not parse \"{}\" as a word", src),
            Err(e) => panic!("Did not parse \"{}\" as a word: {}", src, e),
        }
    }
}

#[test]
fn test_word_escaped_newline_becomes_whitespace() {
    let mut p = make_parser("foo\\\nbar");
    assert_eq!(Ok(Some(word("foo"))), p.word());
    assert_eq!(Ok(Some(word("bar"))), p.word());
}

#[test]
fn test_backticked_valid() {
    let correct = word_subst(ParameterSubstitution::Command(vec!(cmd("foo"))));
    assert_eq!(correct, make_parser("`foo`").backticked_command_substitution().unwrap());
}

#[test]
fn test_backticked_valid_backslashes_removed_if_before_dollar_backslash_and_backtick() {
    let correct = word_subst(ParameterSubstitution::Command(vec!(cmd_from_simple(SimpleCommand {
        vars: vec!(), io: vec!(),
        cmd: Some((word("foo"), vec!(
            TopLevelWord(Concat(vec!(
                Word::Simple(Param(Parameter::Dollar)),
                escaped("`"),
                escaped("o"),
            )))
        ))),
    }))));
    assert_eq!(correct, make_parser("`foo \\$\\$\\\\\\`\\o`").backticked_command_substitution().unwrap());
}

#[test]
fn test_backticked_nested_backticks() {
    let correct = word_subst(ParameterSubstitution::Command(vec!(
        cmd_from_simple(SimpleCommand {
            vars: vec!(), io: vec!(),
            cmd: Some((word("foo"), vec!(
                word_subst(
                    ParameterSubstitution::Command(vec!(cmd_from_simple(SimpleCommand {
                        vars: vec!(), io: vec!(),
                        cmd: Some((word("bar"), vec!(TopLevelWord(Concat(vec!(escaped("$"), escaped("$")))))))
                    })))
                )
            ))),
        })
    )));
    assert_eq!(correct, make_parser(r#"`foo \`bar \\\\$\\\\$\``"#).backticked_command_substitution().unwrap());
}

#[test]
fn test_backticked_nested_backticks_x2() {
    let correct = word_subst(ParameterSubstitution::Command(vec!(
        cmd_from_simple(SimpleCommand {
            vars: vec!(), io: vec!(),
            cmd: Some((word("foo"), vec!(word_subst(
                ParameterSubstitution::Command(vec!(cmd_from_simple(SimpleCommand {
                    vars: vec!(), io: vec!(),
                    cmd: Some((word("bar"), vec!(word_subst(
                        ParameterSubstitution::Command(vec!(cmd_from_simple(SimpleCommand {
                            vars: vec!(), io: vec!(),
                            cmd: Some((word("baz"), vec!(TopLevelWord(Concat(vec!(escaped("$"), escaped("$")))))))
                        })))
                    ))))
                })))
            ))))
        })
    )));
    assert_eq!(correct, make_parser(r#"`foo \`bar \\\`baz \\\\\\\\$\\\\\\\\$ \\\`\``"#).backticked_command_substitution().unwrap());
}

#[test]
fn test_backticked_nested_backticks_x3() {
    let correct = word_subst(ParameterSubstitution::Command(vec!(
        cmd_from_simple(SimpleCommand {
            vars: vec!(), io: vec!(),
            cmd: Some((word("foo"), vec!(word_subst(
                ParameterSubstitution::Command(vec!(cmd_from_simple(SimpleCommand {
                    vars: vec!(), io: vec!(),
                    cmd: Some((word("bar"), vec!(word_subst(
                        ParameterSubstitution::Command(vec!(cmd_from_simple(SimpleCommand {
                            vars: vec!(), io: vec!(),
                            cmd: Some((word("baz"), vec!(word_subst(
                                ParameterSubstitution::Command(vec!(cmd_from_simple(SimpleCommand {
                                    vars: vec!(), io: vec!(),
                                    cmd: Some((word("qux"), vec!(TopLevelWord(Concat(vec!(escaped("$"), escaped("$")))))))
                                })))
                            )))),
                        })))
                    ))))
                })))
            ))))
        })
    )));
    assert_eq!(correct, make_parser(
        r#"`foo \`bar \\\`baz \\\\\\\`qux \\\\\\\\\\\\\\\\$\\\\\\\\\\\\\\\\$ \\\\\\\` \\\`\``"#
    ).backticked_command_substitution().unwrap());
}

#[test]
fn test_backticked_invalid_missing_closing_backtick() {
    let src = [
        // Missing outermost backtick
        (r#"`foo"#, src(0,1,1)),
        (r#"`foo \`bar \\\\$\\\\$\`"#, src(0,1,1)),
        (r#"`foo \`bar \\\`baz \\\\\\\\$\\\\\\\\$ \\\`\`"#, src(0,1,1)),
        (r#"`foo \`bar \\\`baz \\\\\\\`qux \\\\\\\\\\\\\\\\$ \\\\\\\\\\\\\\\\$ \\\\\\\` \\\`\`"#, src(0,1,1)),

        // Missing second to last backtick
        (r#"`foo \`bar \\\\$\\\\$`"#, src(6,1,7)),
        (r#"`foo \`bar \\\`baz \\\\\\\\$\\\\\\\\$ \\\``"#, src(6,1,7)),
        (r#"`foo \`bar \\\`baz \\\\\\\`qux \\\\\\\\\\\\\\\\$ \\\\\\\\\\\\\\\\$ \\\\\\\` \\\``"#, src(6,1,7)),

        // Missing third to last backtick
        (r#"`foo \`bar \\\`baz \\\\\\\\$\\\\\\\\$ \``"#, src(14,1,15)),
        (r#"`foo \`bar \\\`baz \\\\\\\`qux \\\\\\\\\\\\\\\\$ \\\\\\\\\\\\\\\\$ \\\\\\\` \``"#, src(14,1,15)),

        // Missing fourth to last backtick
        (r#"`foo \`bar \\\`baz \\\\\\\`qux \\\\\\\\\\\\\\\\$ \\\\\\\\\\\\\\\\$ \\\`\``"#, src(26,1,27)),
    ];

    for &(s, p) in &src {
        let correct = Unmatched(Token::Backtick, p);
        match make_parser(s).backticked_command_substitution() {
            Ok(w) => panic!("Unexpectedly parsed the source \"{}\" as\n{:?}", s, w),
            Err(ref err) => if err != &correct {
                panic!("Expected the source \"{}\" to return the error `{:?}`, but got `{:?}`",
                       s, correct, err);
            },
        }
    }
}

#[test]
fn test_backticked_invalid_maintains_accurate_source_positions() {
    let src = [
        (r#"`foo ${invalid param}`"#, src(14,1,15)),
        (r#"`foo \`bar ${invalid param}\``"#, src(20,1,21)),
        (r#"`foo \`bar \\\`baz ${invalid param} \\\`\``"#, src(28,1,29)),
        (r#"`foo \`bar \\\`baz \\\\\\\`qux ${invalid param} \\\\\\\` \\\`\``"#, src(40,1,41)),
    ];

    for &(s, p) in &src {
        let correct = BadSubst(Token::Whitespace(String::from(" ")), p);
        match make_parser(s).backticked_command_substitution() {
            Ok(w) => panic!("Unexpectedly parsed the source \"{}\" as\n{:?}", s, w),
            Err(ref err) => if err != &correct {
                panic!("Expected the source \"{}\" to return the error `{:?}`, but got `{:?}`",
                       s, correct, err);
            },
        }
    }
}

#[test]
fn test_backticked_invalid_missing_opening_backtick() {
    let mut p = make_parser("foo`");
    assert_eq!(
        Err(Unexpected(Token::Name(String::from("foo")), src(0,1,1))),
        p.backticked_command_substitution()
    );
}

#[test]
fn ensure_parse_errors_are_send_and_sync() {
    fn send_and_sync<T: Send + Sync>() {}
    send_and_sync::<ParseError<()>>();
}

#[test]
fn ensure_parser_could_be_send_and_sync() {
    fn send_and_sync<T: Send + Sync>() {}
    send_and_sync::<Parser<::std::vec::IntoIter<Token>, StringBuilder>>();
}
