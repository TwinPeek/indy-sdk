use services::crypto::anoncreds::types::{PublicKey, PrimaryEqualProof, PrimaryPredicateGEProof, Predicate, ProofInput, PrimaryProof, FullProof};
use services::crypto::anoncreds::constants::{LARGE_E_START};
use services::crypto::helpers::get_hash_as_int;
use services::crypto::wrappers::bn::BigNumber;
use std::collections::{HashMap, HashSet};
use std::iter::FromIterator;
use errors::crypto::CryptoError;

pub struct Verifier {}

impl Verifier {
    pub fn new() -> Verifier {
        Verifier {}
    }

    pub fn verify(&self, proof_input: &ProofInput, proof: &FullProof, all_revealed_attrs: &HashMap<String, BigNumber>, nonce: &BigNumber) -> Result<bool, CryptoError> {
        let mut tau_list = Vec::new();

        let it = proof.schema_keys.iter().zip(proof.proofs.iter());

        for (i, (schema_key, proof_item)) in it.enumerate() {
            if let Some(ref primary_proof) = proof_item.primary_proof {
                tau_list.append(
                    &mut try!(self.verify_primary_proof(&proof_input, &proof.c_hash, &primary_proof, &all_revealed_attrs))
                )
            }
        }

        let mut values: Vec<BigNumber> = vec![];

        values.push(try!(nonce.clone()));
        values.append(&mut tau_list);

        for el in proof.c_list.iter() {
            values.push(try!(el.clone()));
        }

        let c_hver = try!(get_hash_as_int(&mut values));

        Ok(c_hver == proof.c_hash)
    }

    fn verify_primary_proof(&self, proof_input: &ProofInput, c_hash: &BigNumber, primary_proof: &PrimaryProof,
                            all_revealed_attrs: &HashMap<String, BigNumber>) -> Result<Vec<BigNumber>, CryptoError> {
        let mut t_hat: Vec<BigNumber> =
            try!(self.verify_equality(&primary_proof.eq_proof, &c_hash, &all_revealed_attrs));

        for ge_proof in primary_proof.ge_proofs.iter() {
            t_hat.append(&mut try!(self.verify_ge_predicate(ge_proof, &c_hash)))
        }
        Ok(t_hat)
    }

    fn verify_equality(&self, proof: &PrimaryEqualProof, c_h: &BigNumber, all_revealed_attrs: &HashMap<String, BigNumber>) -> Result<Vec<BigNumber>, CryptoError> {
        let pk: PublicKey = try!(mocks::wallet_get_pk());//TODO:  get from wallet
        let attr_names = vec!["name".to_string(), "age".to_string(), "height".to_string(), "sex".to_string()];//TODO:  get from wallet

        let attr_names_hash_set = HashSet::<String>::from_iter(attr_names.iter().cloned());
        let revealed_attr_names = HashSet::<String>::from_iter(proof.revealed_attr_names.iter().cloned());

        let unrevealed_attr_names =
            attr_names_hash_set
                .difference(&revealed_attr_names)
                .map(|attr| attr.to_owned())
                .collect::<Vec<String>>();

        let t1: BigNumber = try!(self.calc_teq(&pk, &proof.a_prime, &proof.e, &proof.v, &proof.m,
                                               &proof.m1, &proof.m2, &unrevealed_attr_names));

        let mut ctx = try!(BigNumber::new_context());
        let mut rar = try!(BigNumber::from_dec("1"));

        for attr_name in proof.revealed_attr_names.iter() {
            let cur_r = try!(pk.r.get(attr_name)
                .ok_or(CryptoError::BackendError("Element not found".to_string())));
            let cur_attr = try!(all_revealed_attrs.get(attr_name)
                .ok_or(CryptoError::BackendError("Element not found".to_string())));

            rar = try!(
                cur_r
                    .mod_exp(&cur_attr, &pk.n, Some(&mut ctx))?
                    .mul(&rar, Some(&mut ctx))
            );
        }

        let large_e_start = try!(BigNumber::from_dec(&LARGE_E_START.to_string()[..]));

        let tmp: BigNumber = try!(
            BigNumber::from_dec("2")?
                .exp(&large_e_start, Some(&mut ctx))
        );

        rar = try!(
            proof.a_prime
                .mod_exp(&tmp, &pk.n, Some(&mut ctx))?
                .mul(&rar, Some(&mut ctx))
        );

        let t2: BigNumber = try!(
            pk.z
                .mod_div(&rar, &pk.n)?
                .mod_exp(&c_h, &pk.n, Some(&mut ctx))?
                .inverse(&pk.n, Some(&mut ctx))
        );

        let t: BigNumber = try!(
            t1
                .mul(&t2, Some(&mut ctx))?
                .modulus(&pk.n, Some(&mut ctx))
        );

        Ok(vec![t])
    }

    fn verify_ge_predicate(&self, proof: &PrimaryPredicateGEProof, c_h: &BigNumber) -> Result<Vec<BigNumber>, CryptoError> {
        let pk = mocks::wallet_get_pk().unwrap();/////wallet get pk
        let (k, v) = (&proof.predicate.attr_name, &proof.predicate.value);
        let mut tau_list = try!(self.calc_tge(&pk, &proof.u, &proof.r, &proof.mj,
                                              &proof.alpha, &proof.t));
        let mut ctx = try!(BigNumber::new_context());

        for i in 0..4 {
            let cur_t = try!(proof.t.get(&i.to_string()[..])
                .ok_or(CryptoError::BackendError("Element not found".to_string())));

            tau_list[i] =
                try!(
                    cur_t
                        .mod_exp(&c_h, &pk.n, Some(&mut ctx))?
                        .inverse(&pk.n, Some(&mut ctx))?
                        .mul(&tau_list[i], Some(&mut ctx))?
                        .modulus(&pk.n, Some(&mut ctx))
                );
        }

        let big_v = try!(BigNumber::from_dec(&v.to_string()[..]));
        let delta = try!(proof.t.get("DELTA")
            .ok_or(CryptoError::BackendError("Element not found".to_string())));


        tau_list[4] = try!(
            pk.z.mod_exp(&big_v, &pk.n, Some(&mut ctx))?
                .mul(&delta, Some(&mut ctx))?
                .mod_exp(&c_h, &pk.n, Some(&mut ctx))?
                .inverse(&pk.n, Some(&mut ctx))?
                .mul(&tau_list[4], Some(&mut ctx))?
                .modulus(&pk.n, Some(&mut ctx))
        );

        tau_list[5] = try!(
            delta.mod_exp(&c_h, &pk.n, Some(&mut ctx))?
                .inverse(&pk.n, Some(&mut ctx))?
                .mul(&tau_list[5], Some(&mut ctx))?
                .modulus(&pk.n, Some(&mut ctx))
        );

        Ok(tau_list)
    }

    fn calc_tge(&self, pk: &PublicKey, u: &HashMap<String, BigNumber>, r: &HashMap<String,
        BigNumber>, mj: &BigNumber, alpha: &BigNumber, t: &HashMap<String, BigNumber>)
                -> Result<Vec<BigNumber>, CryptoError> {
        let mut tau_list: Vec<BigNumber> = Vec::new();
        let mut ctx = try!(BigNumber::new_context());

        for i in 0..4 {
            let cur_u = try!(u.get(&i.to_string()[..])
                .ok_or(CryptoError::BackendError("Element not found".to_string())));
            let cur_r = try!(r.get(&i.to_string()[..])
                .ok_or(CryptoError::BackendError("Element not found".to_string())));

            let pks_pow_r: BigNumber = try!(pk.s.mod_exp(&cur_r, &pk.n, Some(&mut ctx)));

            let t_tau = try!(
                pk.z
                    .mod_exp(&cur_u, &pk.n, Some(&mut ctx))?
                    .mul(&pks_pow_r, Some(&mut ctx))?
                    .modulus(&pk.n, Some(&mut ctx))
            );

            tau_list.push(t_tau);
        }

        let delta = try!(r.get("DELTA")
            .ok_or(CryptoError::BackendError("Element not found".to_string())));

        let pks_pow_delta = try!(pk.s.mod_exp(&delta, &pk.n, Some(&mut ctx)));

        let t_tau = try!(
            pk.z
                .mod_exp(&mj, &pk.n, Some(&mut ctx))?
                .mul(&pks_pow_delta, Some(&mut ctx))?
                .modulus(&pk.n, Some(&mut ctx))
        );

        tau_list.push(t_tau);

        let mut q: BigNumber = try!(BigNumber::from_dec("1"));

        for i in 0..4 {
            let cur_t = try!(t.get(&i.to_string()[..])
                .ok_or(CryptoError::BackendError("Element not found".to_string())));
            let cur_u = try!(u.get(&i.to_string()[..])
                .ok_or(CryptoError::BackendError("Element not found".to_string())));

            q = try!(
                cur_t
                    .mod_exp(&cur_u, &pk.n, Some(&mut ctx))?
                    .mul(&q, Some(&mut ctx))
            );
        }

        q = try!(
            pk.s
                .mod_exp(&alpha, &pk.n, Some(&mut ctx))?
                .mul(&q, Some(&mut ctx))?
                .modulus(&pk.n, Some(&mut ctx))
        );

        tau_list.push(q);

        Ok(tau_list)
    }

    fn calc_teq(&self, pk: &PublicKey, a_prime: &BigNumber, e: &BigNumber, v: &BigNumber,
                mtilde: &HashMap<String, BigNumber>, m1tilde: &BigNumber, m2tilde: &BigNumber,
                unrevealed_attr_names: &Vec<String>) -> Result<BigNumber, CryptoError> {
        let mut result: BigNumber = try!(BigNumber::from_dec("1"));
        let tmp: BigNumber = try!(BigNumber::new());
        let mut ctx = try!(BigNumber::new_context());

        for k in unrevealed_attr_names.iter() {
            let cur_r = try!(pk.r.get(k)
                .ok_or(CryptoError::BackendError("Element not found".to_string())));
            let cur_m = try!(mtilde.get(k)
                .ok_or(CryptoError::BackendError("Element not found".to_string())));

            result = try!(cur_r
                .mod_exp(&cur_m, &pk.n, Some(&mut ctx))?
                .mul(&result, Some(&mut ctx))
            );
        }

        result = try!(
            pk.rms
                .mod_exp(&m1tilde, &pk.n, Some(&mut ctx))?
                .mul(&result, Some(&mut ctx))
        );

        result = try!(
            pk.rctxt
                .mod_exp(&m2tilde, &pk.n, Some(&mut ctx))?
                .mul(&result, Some(&mut ctx))
        );

        result = try!(
            a_prime
                .mod_exp(&e, &pk.n, Some(&mut ctx))?
                .mul(&result, Some(&mut ctx))
        );

        result = try!(
            pk.s
                .mod_exp(&v, &pk.n, Some(&mut ctx))?
                .mul(&result, Some(&mut ctx))?
                .modulus(&pk.n, Some(&mut ctx))
        );

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use services::crypto::anoncreds::types::{SchemaKey, Proof};

    #[test]
    fn verify_test() {
        let verifier = Verifier::new();

        let mut all_revealed_attrs = HashMap::new();
        all_revealed_attrs.insert("name".to_string(), BigNumber::from_dec("1139481716457488690172217916278103335").unwrap());

        let nonce = BigNumber::from_dec("150136900874297269339868").unwrap();

        let predicate = Predicate { attr_name: "age".to_string(), p_type: "ge".to_string(), value: 18 };

        let proof_input = ProofInput {
            revealed_attrs: vec!["name".to_string()],
            predicates: vec![predicate],
            ts: "".to_string(),
            pubseq_no: "".to_string()
        };
        let schema_key = SchemaKey { name: "GVT".to_string(), version: "1.0".to_string(), issue_id: "issuer1".to_string() };

        let eq_proof = mocks::get_eq_proof().unwrap();
        let ge_proof = mocks::get_ge_proof().unwrap();

        let primary_proof = PrimaryProof {
            eq_proof: eq_proof,
            ge_proofs: vec![ge_proof]
        };

        let proof = Proof {
            primary_proof: Some(primary_proof)
        };

        let proof = FullProof {
            c_hash: BigNumber::from_dec("90321426117300366618517575493200873441415194969656589575988281157859869553034").unwrap(),
            schema_keys: vec![schema_key],
            proofs: vec![proof],
            c_list: vec![]
        };

        let res = verifier.verify(
            &proof_input,
            &proof,
            &all_revealed_attrs,
            &nonce
        );

        assert!(res.is_ok());
        assert_eq!(false, res.unwrap());//TODO replace it on true after implementation verify non revocation proof
    }

    #[test]
    fn verify_equlity_test() {
        let verifier = Verifier::new();
        let proof = mocks::get_eq_proof().unwrap();
        let c_h = BigNumber::from_dec("90321426117300366618517575493200873441415194969656589575988281157859869553034").unwrap();

        let mut all_revealed_attrs = HashMap::new();
        all_revealed_attrs.insert("name".to_string(), BigNumber::from_dec("1139481716457488690172217916278103335").unwrap());

        let res: Result<Vec<BigNumber>, CryptoError> = verifier.verify_equality(
            &proof,
            &c_h,
            &all_revealed_attrs
        );

        assert!(res.is_ok());
        assert_eq!("8587651374942675536728753067347608709923065423222685438966198646355384235605146057750016685007100765028881800702364440231217947350369743\
    7857804979183199263295761778145588965111459517594719543696782791489766042732025814161437109818972963936021789845879318003605961256519820582781422914\
    97483852459936553097915975160943885654662856194246459692268230399812271607008648333989067502873781526028636897730244216695340964909830792881918581540\
    43873141931971315451530757661716555801069654237014399171221318077704626190288641508984014104319842941642570762210967615676477710700081132170451096239\
    93976701236193875603478579771137394", res.unwrap()[0].to_dec().unwrap());
    }

    #[test]
    fn verify_ge_predicate_works() {
        let verifier = Verifier::new();
        let proof = mocks::get_ge_proof().unwrap();
        let c_h = BigNumber::from_dec("90321426117300366618517575493200873441415194969656589575988281157859869553034").unwrap();

        let res = verifier.verify_ge_predicate(&proof, &c_h);

        assert!(res.is_ok());
        let res_data = res.unwrap();

        assert_eq!("590677196901723818020415922807296116426887937783467552329163347868728175050285426810380554550521915469309366010293784655561646989461816914001376856160959474\
    724414209525842689549578189455824659628722854086979862112126227427503673036934175777141430158851152801070493790103722897828582782870163648640848483116640936376249697914\
    633137312593554018309295958591096901852088786667038390724116720409279123241545342232722741939277853790638731624274772561371001348651265045334956091681420778381377735879\
    68669689592641726487646825879342092157114737380151398135267202044295696236084701682251092338479916535603864922996074284941502", res_data[0].to_dec().unwrap());

        assert_eq!("543920569174455471552712599639581440766547705711484869326147123041712949811245262311199901062814754524825877546701435180039685252325466998614308056075575752\
    3012229141304994213488418248472205210074847942832434112795278331835277383464971076923322954858384250535611705097886772449075174912745310975145629869588136613587711321262\
    7728458751804045531877233822168791389059182616293449039452340074699209366938385424160688825799810090127647002083194688148464107036527938948376814931919821538192884074388\
    857130767228996607411418624748269121453442291957717517888961515288426522014549478484314078535183196345054464060687989571272", res_data[4].to_dec().unwrap());

        assert_eq!("5291248239406641292396471233645296793027806694289670593845325691604331838238498977162512644007769726817609527208308190348307854043130390623053807510337254881\
    53385441651181164838096995680599793153167424540679236858880383788178608357393234960916139159480841866618336282250341768534336113015828670517732010317195575756736857228019\
    99959821781284558791752968988627903716556541708694042188547572928871840445046338355043889462205730182388607688269913628444534146082714639049648123224230863440138867623776\
    549927089094790233964941899325435455174972634582611070515233787127321158133866337540066814079592094148393576048620611972", res_data[5].to_dec().unwrap());
    }

    #[test]
    fn calc_teg_works() {
        let verifier = Verifier::new();
        let proof = mocks::get_ge_proof().unwrap();
        let pk = mocks::wallet_get_pk().unwrap();

        let res = verifier.calc_tge(&pk, &proof.u, &proof.r, &proof.mj,
                                    &proof.alpha, &proof.t);

        assert!(res.is_ok());

        let res_data = res.unwrap();

        assert_eq!("66763809913905005196685504127801735117197865238790458248607529048879049233469065301125917408730585682472169276319924014654607203248656655401523177550968\
    79005126037514992260570317766093693503820466315473651774235097627461187468560528498637265821197064092074734183979312736841571077239362785443096285343022325743749493\
    115671111253247628251990871764988964166665374208195759750683082601207244879323795625125414213912754126587933035233507317880982815199471233315480695428246221116099530\
    2762582265012461801281742135973017791914100890332877707316728113640973774147232476482160263443368393229756851203511677358619849710094360", res_data[1].to_dec().unwrap());

        assert_eq!("1696893728060613826189455641919714506779750280465195946299906248745222420050846334948115499804146149236210969719663609022008928047696210368681129164314195\
    73961162181255619271925974300906611593381407468871521942852472844008029827907111131222578449896833731023679346466149116169563017889291210126870245249099669006944487937\
    701186090023854916946824876428968293209784770081426960793331644949561007921128739917551308870397017309196194046088818137669808278548338892856171583731467477794490146449\
    84371272994658213772000759824325978473230458194532365204418256638583185120380190225687161021928828234401021859449125311307071", res_data[4].to_dec().unwrap());

        assert_eq!("7393309861349259392630193573257336708857960195548821598928169647822585190694497646718777350819780512754931147438702100908573008083971392605400292392558068639\
    6426790932973170010764749286999115602174793097294839591793292822808780386838139840847178284597133066509806751359097256406292722692372335587138313303601933346125677119170\
    3745548456402537166527941377105628418709499120225110517191272248627626095292045349794519230242306378755919873322083068080833514101587864250782718259987761547941791394977\
    87217811540121982252785628801722587508068009691576296044178037535833166612637915579540102026829676380055826672922204922443", res_data[5].to_dec().unwrap());
    }

    #[test]
    fn calc_teq_works() {
        let verifier = Verifier::new();
        let proof = mocks::get_eq_proof().unwrap();
        let pk = mocks::wallet_get_pk().unwrap();

        let res = verifier.calc_teq(&pk, &proof.a_prime, &proof.e, &proof.v,
                                    &proof.m, &proof.m1, &proof.m2,
                                    &vec!["sex".to_string(), "age".to_string(), "height".to_string()]
        );

        assert!(res.is_ok());
        assert_eq!("44674566012490574873221338726897300898913972309497258940219569980165585727901128041268469063382008728753943624549705899352321456091543114868302412585283526922\
    48482588030725250950307379112600430281021015407801054038315353187338898917957982724509886210242668120433945426431434030155726888483222722925281121829536918755833970204795\
    18277688063064207469055405971871717892031608853055468434231459862469415223592109268515989593021324862858241499053669862628606497232449247691824831224716135821088977103328\
    37686070090582706144278719293684893116662729424191599602937927245245078018737281020133694291784582308345229012480867237", res.unwrap().to_dec().unwrap());
    }
}

mod mocks {
    use super::*;

    pub fn wallet_get_pk() -> Result<PublicKey, CryptoError> {
        let mut r = HashMap::new();
        r.insert("name".to_string(), try!(BigNumber::from_dec("55636937636844819812189791288187243913404055721058334520072574568680438360936320682628189506248931475232504868784141809162526982794777886937554791279646171992316154768489491205932973020390955775825994246509354890417980543491344959419958264200222321573290332068573840656874584148318471805081070819330139498643368112616125508016850665039138240007045133711819182960399913468566074586611076818097815310939823561848962949647054263397457358507697316036204724311688330058092618087260011626918624130336633163118234963001890740389604366796070789463043007475519162863457847133916866147682877703700016314519649272629853810342756")));
        r.insert("height".to_string(), try!(BigNumber::from_dec("32014206266070285395118493698246684536543402308857326229844369749153998025988120078148833919040926762489849787174726278317154939222455553684674979640533728771798727404529140716275948809394914126446467274094766630776034154814466245563241594664595503357965283703581353868787640425189228669159837529621065262578472511140258233443082035493432067002995028424708181638248338655901732889892559561796172833245307347288440850886016760883963087954594369665160758244185860669353304463245326602784567519981372129418674907732019485821481470791951576038671383506105840172336020165255666872489673679749492975692222529386986002548508")));
        r.insert("age".to_string(), try!(BigNumber::from_dec("5573886601587513393941805393558438475134278869721908377896820376573868172897985632537697650826768061917733566546691785934393119648542993289296693181509209448802827620254572500988956963540401872482092959068516484681223765164669694589952326903719257213107559712016680752042520470482095682948519795635218252370953948099226141669796718651544648226881826585169101432801215379161624527044414118535373924688074790569833168081423701512430033511620744395776217769497965549575153091462845485986562792539143519413414753164756782101386489471333391388468474082175228293592033872018644198196278046021752128670441648674265160079365")));
        r.insert("sex".to_string(), try!(BigNumber::from_dec("44319112097252841415305877008967513656231862316131581238409828513703699212059952418622049664178569730633939544882861264006945675755509881864438312327074402062963599178195087536260752294006450133601248863198870283839961116512248865885787100775903023034879852152846002669257161013317472827548494571935048240800817870893700771269978535707078640961353407573194897812343272563394036737677668293122931520603798620428922052839619195929427039933665104815440476791376703125056734891504425929510493567119107731184250744646520780647416583157402277832961026300695141515177928171182043898138863324570665593349095177082259229019129")));

        let n = try!(BigNumber::from_dec("95230844261716231334966278654105782744493078250034916428724307571481648650972254096365233503303500776910009532385733941342231244809050180342216701303297309484964627111488667613567243812137828734726055835536190375874228378361894062875040911721595668269426387378524841651770329520854646198182993599992246846197622806018586940960824812499707703407200235006250330376435395757240807360245145895448238973940748414130249165698642798758094515234629492123379833360060582377815656998861873479266942101526163937107816424422201955494796734174781894506437514751553369884508767256335322189421050651814494097369702888544056010606733"));
        let s = try!(BigNumber::from_dec("83608735581956052060766602122241456047092927591272898317077507857903324472083195301035502442829713523495655160192410742120440247481077060649728889735943333622709039987090137325037494001551239812739256925595650405403616377574225590614582056226657979932825031688262428848508620618206304014287232713708048427099425348438343473342088258502098208531627321778163620061043269821806176268690486341352405206188888371253713940995260309747672937693391957731544958179245054768704977202091642139481745073141174316305851938990898215928942632876267309335084279137046749673230694376359278715909536580114502953378593787412958122696491"));
        let rms = try!(BigNumber::from_dec("12002410972675035848706631786298987049295298281772467607461994087192649160666347028767622091944497528304565759377490497287538655369597530498218287879384450121974605678051982553150980093839175365101087722528582689341030912237571526676430070213849160857477430406424356131111577547636360346507596843363617776545054084329725294982409132506989181200852351104199115448152798956456818387289142907618956667090125913885442746763678284193811934837479547315881192351556311788630337391374089308234091189363160599574268958752271955343795665269131980077642259235693653829664040302092446308732796745472579352704501330580826351662240"));
        let rctxt = try!(BigNumber::from_dec("77129119521935975385795386930301402827628026853991528755303486255023263353142617098662225360498227999564663438861313570702364984107826653399214544314002820732458443871729599318191904265844432709910182014204478532265518566229953111318413830009256162339443077098917698777223763712267731802804425167444165048596271025553618253855465562660530445682078873631967934956107222619891473818051441942768338388425312823594456990243766677728754477201176089151138798586336262283249409402074987943625960454785501038059209634637204497573094989557296328178873844804605590768348774565136642366470996059740224170274762372312531963184654"));
        let z = try!(BigNumber::from_dec("55164544925922114758373643773121488212903100773688663772257168750760838562077540114734459902014369305346806516101767509487128278169584105585138623374643674838487232408713159693511105298301789373764578281065365292802332455328842835614608027129883137292324033168485729810074426971615144489078436563295402449746541981155232849178606822309310700682675942602404109375598809372735287212196379089816519481644996930522775604565458855945697714216633192192613598668941671920105596720544264146532180330974698466182799108850159851058132630467033919618658033816306014912309279430724013987717126519405488323062369100827358874261055"));

        Ok(PublicKey { n: n, r: r, s: s, rms: rms, rctxt: rctxt, z: z })
    }

    pub fn get_ge_proof() -> Result<PrimaryPredicateGEProof, CryptoError> {
        let mut u = HashMap::new();
        u.insert("3".to_string(), try!(BigNumber::from_dec("8991055448884746937183597583722774762484126625050383332471998457846949141029373442125727754282056746716432451682903479769768810979073516373079900011730658561904955804441830070201")));
        u.insert("0".to_string(), try!(BigNumber::from_dec("3119202262454581234238204378430624579411334710168862570697460713017731159978676020931526979958444245337314728482384630008014840583008894200291024490955989484910144381416270825034")));
        u.insert("1".to_string(), try!(BigNumber::from_dec("15518000836072591312584487513042312668531396837108384118443738039943502537464561749838550874453205824891384223838670020857450197084265206790593562375607300810229831781795248272746")));
        u.insert("2".to_string(), try!(BigNumber::from_dec("14825520448375036868008852928056676407055827587737481734442472562914657791730493564843449537953640698472823089255666508559183853195339338542320239187247714921656011972820165680495")));

        let mut r = HashMap::new();
        r.insert("3".to_string(), try!(BigNumber::from_dec("1167550272049401879986208522893402310804598464734091634200466392129423083223947805081084530528884868358954909996620252475186022489983411045778042594227739715134711989282499524985320110488413880945529981664361709639820806122583682452503036404728763373201248045691893015110010852379757063328461525233426468857514764722036069158904178265410282906843586731152479716245390735227750422991960772359397820443448680191460821952125514509645145886564188922269624264085160475580804514964397619916759653999513671049924196777087113468144988512960417719152393266552894992285322714901696251664710315454136548433461200202231002410586808552657105706728516271798034029334358544147228606049936435037524531381025620665456890088546982587481")));
        r.insert("0".to_string(), try!(BigNumber::from_dec("2171447327600461898681893459994311075091382696626274737692544709852701253236804421376958076382402020619134253300345593917220742679092835017076022500855973864844382438540332185636399240848767743775256306580762848493986046436797334807658055576925997185840670777012790272251814692816605648587784323426613630301003579746571336649678357714763941128273025862159957664671610945626170382202342056873023285304345808387951726158704872306035900016749011783867480420800998854987117527975876541158475438393405152741773026550341616888761476445877989444379785612563226680131486775899233053750237483379057705217586225573410360257816090005804925119313735493995305192861301036330809025262997449946935113898554709938543261959225374477075")));
        r.insert("1".to_string(), try!(BigNumber::from_dec("3407533923994509079922445260572851360802767657194628749769491907793892136495870984243826839220225896118619529161581266999433926347085222629115870923342232719053144390143744050810102224808038416215236832553566711013172199073782742820257909889682618205836240882137941793761945944591631439539425000764465713533076522478368670386820666288924406010336355943518262201405259934614234952964126592210374867434305756945477124161456667354597660261751805125868686764527511228958421917556551368867158045859243933424656693853034751832910802366824624573129457523599814696599411287253040266911475142776766859495751666393668865554821250239426074473894708324406330875647014186109228413419914784738994090638263427510209053496949212198772")));
        r.insert("2".to_string(), try!(BigNumber::from_dec("376615807259433852994889736265571130722120467111857816971887754558663859714462971707188421230515343999387984197735177426886431376277830270779207802969001925574986158648382233404297833366166880771649557924045749558608142093651421705548864007094298410821850827506796116657011958581079961108367131644360333951829519859638856960948927313849945546613528932570789799649277584112030378539271377025534526299113938027086859429617232980159899286261874751664992426761978572712284693482352940080544009977987614687886895144698432208930945866456583811087222056104304977238806342842107136621744373848258397836622192179796587657390442772422614921141854089119770642649923852479045626615424086862226766993260016650650800970901479317353")));
        r.insert("DELTA".to_string(), try!(BigNumber::from_dec("1204576405206979680375064721017725873269565442920750053860275824473279578144966505696401529388362488618656880602103746663719014543804181028271885056878992356241850630746057861156554344680578591346709669594164380854748723108090171168846365315480163847141547319673663867587891086140001578226570294284600635554860177021112021218221677503541742648400417051405848715777401449235718828129001371122909809318916605795606301174787694751963509104301818268975054567300992103690013595997066100692742805505022623908866248955309724353017333598591476683281090839126513676860390307767387899158218974766900357521082392372102989396002839389060003178573720443299965136555923047732519831454019881161819607825392645740545819410001935871296")));

        let mut t = HashMap::new();
        t.insert("3".to_string(), try!(BigNumber::from_dec("83832511302317350174644720338005868487742959910398469815023175597193018639890917887543705415062101786582256768017066777905945250455529792569435063542128440269870355757494523489777576305013971151020301795930610571616963448640783534486881066519012584090452409312729129595716959074161404190572673909049999235573789134838668875246480910001667440875590464739356588846924490130540148723881221509872798683154070397912008198847917146244304739030407870533464478489905826281941434008283229667189082264792381734035454956041612257154896426092221951083981809288053249503709950518771668342922637895684467584044654762057518028814700")));
        t.insert("0".to_string(), try!(BigNumber::from_dec("17363331019061087402844209719893765371888392521507799534029693411314419650156431062459421604096282340039952269582687900721960971874670054761709293109949110830813780630203308029471950250261299362249372820231198558841826592697963838759408960504585788309222390217432925946851327016608993387530098618165007004227557481762160406061606398711655197702267307202795893150693539328844268725519498759780370661097817433632221804533430784357877040495807116168272952720860492630103774088576448694803769740862452948066783609506217979920299119838909533940158375124964345812560749245376080673497973923586841616454700487914362471202008")));
        t.insert("1".to_string(), try!(BigNumber::from_dec("89455656994262898696010620361749819360237582245028725962970005737051728267174145415488622733389621460891337449519650169354661297765474368093442921019918627430103490796403713184394321040862188347280121162030527387297914106124615295029438860483643206878385030782115461217026682705339179345799048771007488017061121097664849533202200732993683759185652675229998618989002320091590048075901070991065565826421958646807185596723738384036684650647137579559949478266162844209656689415344016818360348356312264086908726131174312873340317036154962789954493075076421104496622960243079994511377273760209424275802376704240224057017113")));
        t.insert("2".to_string(), try!(BigNumber::from_dec("89410264446544582460783108256046283919076319065430050325756614584399852372030797406836188839188658589044450904082852710142004660134924756488845128162391217899779712577616690285325130344040888345830793786702389605089886670947913310987447937415013394798653152944186602375622211523989869906842514688368412364643177924764258301720702233619449643601070324239497432310281518069485140179427484578654078080286588210649780194784918635633853990818152978680101738950391705291308278990621417475783919318775532419526399483870315453680012214346133208277396870767376190499172447005639213621681954563685885258611100453847030057210573")));
        t.insert("DELTA".to_string(), try!(BigNumber::from_dec("17531299058220149467416854489421567897910338960471902975273408583568522392255499968302116890306524687486663687730044248160210339238863476091064742601815037120574733471494286906058476822621292173298642666511349405172455078979126802123773531891625097004911163338483230811323704803366602873408421785889893292223666425119841459293545405397943817131052036368166012943639154162916778629230509814424319368937759879498990977728770262630904002681927411874415760739538041907804807946503694675967291621468790462606280423096949972217261933741626487585406950575711867888842552544895574858154723208928052348208022999454364836959913")));

        let predicate = Predicate { attr_name: "age".to_string(), p_type: "ge".to_string(), value: 18 };

        let mj = try!(BigNumber::from_dec("1603425011106247404410993992231356816212687443774810147917707956054468639246061842660922922638282972213339086692783888162583747872610530439675358599658842676000681975294259033921"));
        let alpha = try!(BigNumber::from_dec("10356391427643160498096100322044181597098497015522243313140952718701540840206124784483254227685815326973121415131868716208997744531667356503588945389793642286002145762891552961662804737699174847630739288154243345749050494830443436382280881466833601915627397601315033369264534756381669075511238130934450573103942299767277725603498732898775126784825329479233488928873905649944203334284969529288341712039042121593832892633719941366126598676503928077684908261211960615121039788257179455497199714100480379742080080363623749544442225600170310016965613238530651846654311018291673656192911252359090044631268913200633654215640107245506757349629342277896334140999154991920063754025485899126293818842601918101509689122011832619551509675197082794490012616416413823359927604558553776550532965415598441778103806673039612795460783658848060332784778084904"));

        Ok(PrimaryPredicateGEProof { u: u, r: r, mj: mj, alpha: alpha, t: t, predicate: predicate })
    }

    pub fn get_eq_proof() -> Result<PrimaryEqualProof, CryptoError> {
        let mut mtilde = HashMap::new();
        mtilde.insert("height".to_string(), try!(BigNumber::from_dec("3373978431761662936864523680216977257584610980616339878140476966372383023266465253136551434714889555651032143048543421334122669369824546771790431199967902091704924294162747998714")));
        mtilde.insert("age".to_string(), try!(BigNumber::from_dec("2976250595835739181594320238227653601426197318110939190760657852629456864395726135468275792741622450579986141053384483916124587493975756840689906672199964644984465423799113422915")));
        mtilde.insert("sex".to_string(), try!(BigNumber::from_dec("1038496187132038951426769629254464579084684144036750642303206209710591608223417014007881207499688569061414518819199568509614376078846399946097722727271077857527181666924731796053")));

        let predicate = Predicate { attr_name: "age".to_string(), p_type: "ge".to_string(), value: 18 };

        let a_prime = try!(BigNumber::from_dec("78844788312843933904888269033662162831422304046107077675905006898972188325961502973244613809697759885634089891809903260596596204050337720745582204425029325009022804719252242584040122299621227721199828176761231376551096458193462372191787196647068079526052265156928268144134736182005375490381484557881773286686542404542426808122757946974594449826818670853550143124991683881881113838215414675622341721941313438212584005249213398724981821052915678073798488388669906236343688340695052465960401053524210111298793496466799018612997781887930492163394165793209802065308672404407680589643793898593773957386855704715017263075623"));
        let e = try!(BigNumber::from_dec("157211048330804559357890763556004205033325190265048652432262377822213198765450524518019378474079954420822601420627089523829180910221666161"));
        let v = try!(BigNumber::from_dec("1284941348270882857396668346831283261477214348763690683497348697824290862398878189368957036860440621466109067749261102013043934190657143812489958705080669016032522931660500036446733706678652522515950127754450934645211652056136276859874236807975473521456606914069014082991239036433172213010731627604460900655694372427254286535318919513622655843830315487127605220061147693872530746405109346050119002875962452785135042012369674224406878631029359470440107271769428236320166308531422754837805075091788368691034173422556029573001095280381990063052098520390497628832466059617626095893334305279839243726801057118958286768204379145955518934076042328930415723280186456582783477760604150368095698975266693968743996433862121883506028239575396951810130540073342769017977933561136433479399747016313456753154246044046173236103107056336293744927119766084120338151498135676089834463415910355744516788140991012773923718618015121004759889110"));
        let m1 = try!(BigNumber::from_dec("113866224097885880522899498541789692895180427088521824413896638850295809029417413411152277496349590174605786763072969787168775556353363043323193169646869348691540567047982131578875798814721573306665422753535462043941706296398687162611874398835403372887990167434056141368901284989978738291863881602850122461103"));
        let m2 = try!(BigNumber::from_dec("1323766290428560718316650362032141006992517904653586088737644821361547649912995176966509589375485991923219004461467056332846596210374933277433111217288600965656096366761598274718188430661014172306546555075331860671882382331826185116501265994994392187563331774320231157973439421596164605280733821402123058645"));


        Ok(PrimaryEqualProof {
            revealed_attr_names: vec!["name".to_string()],
            a_prime: a_prime,
            e: e,
            v: v,
            m: mtilde,
            m1: m1,
            m2: m2
        })
    }
}